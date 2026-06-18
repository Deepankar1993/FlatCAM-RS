//! Aperture macro (`AM`) evaluation.
//!
//! Supports the common macro primitives — circle (1), outline (4), polygon (5),
//! vector line (2/20), centre line (21), lower-left line (22) — plus the macro
//! variable/arithmetic mini-language (`$n`, `+ - x /`, parentheses) that real
//! macros (e.g. KiCad rounded rectangles) rely on.

use fc_geo::{circle, regular_polygon, union};
use geo::{Coord, LineString, MultiPolygon, Polygon};

/// A parsed macro definition: the raw primitive statements, evaluated lazily
/// with the per-flash modifier values.
#[derive(Clone, Debug)]
pub struct MacroDef {
    statements: Vec<String>,
}

impl MacroDef {
    pub fn new(statements: Vec<String>) -> Self {
        MacroDef { statements }
    }

    /// Evaluate the macro with the given modifiers, returning geometry centred
    /// at the origin (the caller translates it to the flash location).
    pub fn evaluate(&self, mods: &[f64], steps: usize) -> MultiPolygon<f64> {
        let mut vars: Vec<f64> = Vec::new();
        // $1.. map to modifiers (1-indexed in Gerber).
        for (k, m) in mods.iter().enumerate() {
            set_var(&mut vars, k + 1, *m);
        }

        let mut result = MultiPolygon::new(vec![]);
        for stmt in &self.statements {
            let stmt = stmt.trim();
            if stmt.is_empty() || stmt.starts_with('0') {
                continue; // comment
            }
            if let Some(eq) = stmt.find('=') {
                if stmt.starts_with('$') {
                    let idx: usize = stmt[1..eq].trim().parse().unwrap_or(0);
                    let val = eval_expr(stmt[eq + 1..].trim(), &vars);
                    set_var(&mut vars, idx, val);
                    continue;
                }
            }
            let args: Vec<f64> = stmt
                .split(',')
                .map(|a| eval_expr(a.trim(), &vars))
                .collect();
            if args.is_empty() {
                continue;
            }
            let code = args[0].round() as i32;
            let (geo, exposure) = eval_primitive(code, &args, steps);
            if let Some(geo) = geo {
                if exposure {
                    result = union(&result, &geo);
                } else {
                    result = fc_geo::difference(&result, &geo);
                }
            }
        }
        result
    }
}

fn set_var(vars: &mut Vec<f64>, idx: usize, val: f64) {
    if idx == 0 {
        return;
    }
    if vars.len() < idx {
        vars.resize(idx, 0.0);
    }
    vars[idx - 1] = val;
}

fn get_var(vars: &[f64], idx: usize) -> f64 {
    if idx == 0 || idx > vars.len() {
        0.0
    } else {
        vars[idx - 1]
    }
}

fn eval_primitive(code: i32, a: &[f64], steps: usize) -> (Option<MultiPolygon<f64>>, bool) {
    let arg = |i: usize| a.get(i).copied().unwrap_or(0.0);
    match code {
        1 => {
            // circle: exposure, diameter, x, y[, rotation]
            let exposure = arg(1) != 0.0;
            let dia = arg(2);
            let (x, y) = (arg(3), arg(4));
            (Some(MultiPolygon::new(vec![circle(x, y, dia / 2.0, steps)])), exposure)
        }
        2 | 20 => {
            // vector line: exposure, width, xs, ys, xe, ye, rotation
            let exposure = arg(1) != 0.0;
            let width = arg(2);
            let (xs, ys, xe, ye) = (arg(3), arg(4), arg(5), arg(6));
            let rot = arg(7);
            let pts = vec![Coord { x: xs, y: ys }, Coord { x: xe, y: ye }];
            let mut mp = fc_geo::buffer_path(&pts, width / 2.0, steps);
            mp = rotate(mp, rot);
            (Some(mp), exposure)
        }
        21 => {
            // center line: exposure, width, height, x, y, rotation
            let exposure = arg(1) != 0.0;
            let (w, h, x, y, rot) = (arg(2), arg(3), arg(4), arg(5), arg(6));
            let mp = MultiPolygon::new(vec![fc_geo::centered_rect(x, y, w, h)]);
            (Some(rotate(mp, rot)), exposure)
        }
        22 => {
            // lower-left line: exposure, width, height, x, y, rotation
            let exposure = arg(1) != 0.0;
            let (w, h, x, y, rot) = (arg(2), arg(3), arg(4), arg(5), arg(6));
            let mp = MultiPolygon::new(vec![fc_geo::centered_rect(x + w / 2.0, y + h / 2.0, w, h)]);
            (Some(rotate(mp, rot)), exposure)
        }
        4 => {
            // outline: exposure, n, x0, y0, x1, y1, ... , rotation
            let exposure = arg(1) != 0.0;
            let n = arg(2) as usize;
            let mut ring = Vec::with_capacity(n + 1);
            for k in 0..=n {
                ring.push(Coord { x: arg(3 + 2 * k), y: arg(4 + 2 * k) });
            }
            if ring.first() != ring.last() {
                if let Some(f) = ring.first().copied() {
                    ring.push(f);
                }
            }
            let rot = arg(3 + 2 * (n + 1));
            let poly = Polygon::new(LineString::new(ring), vec![]);
            let mp = rotate(MultiPolygon::new(vec![poly]), rot);
            (Some(mp), exposure)
        }
        5 => {
            // polygon: exposure, n vertices, x, y, diameter, rotation
            let exposure = arg(1) != 0.0;
            let nv = arg(2) as usize;
            let (x, y, dia, rot) = (arg(3), arg(4), arg(5), arg(6));
            (
                Some(MultiPolygon::new(vec![regular_polygon(x, y, dia, nv.max(3), rot)])),
                exposure,
            )
        }
        6 => (Some(crate::macro_primitives::moire(a, steps)), true),
        7 => (Some(crate::macro_primitives::thermal(a, steps)), true),
        _ => (None, true),
    }
}

fn rotate(mp: MultiPolygon<f64>, deg: f64) -> MultiPolygon<f64> {
    if deg == 0.0 {
        return mp;
    }
    use geo::AffineOps;
    let t = geo::AffineTransform::rotate(deg, Coord { x: 0.0, y: 0.0 });
    mp.affine_transform(&t)
}

// ----- tiny expression evaluator: + - x X / ( ) and $n -----
fn eval_expr(s: &str, vars: &[f64]) -> f64 {
    let tokens = lex(s);
    let mut p = ExprParser { tokens, pos: 0, vars };
    p.expr()
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(f64),
    Var(usize),
    Plus,
    Minus,
    Mul,
    Div,
    LParen,
    RParen,
}

fn lex(s: &str) -> Vec<Tok> {
    let mut out = Vec::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            ' ' | '\t' => i += 1,
            '+' => { out.push(Tok::Plus); i += 1; }
            '-' => { out.push(Tok::Minus); i += 1; }
            'x' | 'X' => { out.push(Tok::Mul); i += 1; }
            '/' => { out.push(Tok::Div); i += 1; }
            '(' => { out.push(Tok::LParen); i += 1; }
            ')' => { out.push(Tok::RParen); i += 1; }
            '$' => {
                i += 1;
                let mut num = String::new();
                while i < chars.len() && chars[i].is_ascii_digit() {
                    num.push(chars[i]);
                    i += 1;
                }
                out.push(Tok::Var(num.parse().unwrap_or(0)));
            }
            d if d.is_ascii_digit() || d == '.' => {
                let mut num = String::new();
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    num.push(chars[i]);
                    i += 1;
                }
                out.push(Tok::Num(num.parse().unwrap_or(0.0)));
            }
            _ => i += 1,
        }
    }
    out
}

struct ExprParser<'a> {
    tokens: Vec<Tok>,
    pos: usize,
    vars: &'a [f64],
}

impl<'a> ExprParser<'a> {
    fn peek(&self) -> Option<&Tok> {
        self.tokens.get(self.pos)
    }
    fn next(&mut self) -> Option<Tok> {
        let t = self.tokens.get(self.pos).cloned();
        self.pos += 1;
        t
    }
    fn expr(&mut self) -> f64 {
        let mut v = self.term();
        while let Some(t) = self.peek() {
            match t {
                Tok::Plus => { self.next(); v += self.term(); }
                Tok::Minus => { self.next(); v -= self.term(); }
                _ => break,
            }
        }
        v
    }
    fn term(&mut self) -> f64 {
        let mut v = self.factor();
        while let Some(t) = self.peek() {
            match t {
                Tok::Mul => { self.next(); v *= self.factor(); }
                Tok::Div => {
                    self.next();
                    let d = self.factor();
                    // Avoid producing inf/NaN geometry from a zero divisor.
                    if d != 0.0 {
                        v /= d;
                    }
                }
                _ => break,
            }
        }
        v
    }
    fn factor(&mut self) -> f64 {
        match self.next() {
            Some(Tok::Num(n)) => n,
            Some(Tok::Var(i)) => get_var(self.vars, i),
            Some(Tok::Minus) => -self.factor(),
            Some(Tok::Plus) => self.factor(),
            Some(Tok::LParen) => {
                let v = self.expr();
                if self.peek() == Some(&Tok::RParen) {
                    self.next();
                }
                v
            }
            _ => 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expr_arithmetic() {
        let vars = vec![2.0, 3.0];
        assert_eq!(eval_expr("1+2x3", &vars), 7.0);
        assert_eq!(eval_expr("$1x$2", &vars), 6.0);
        assert_eq!(eval_expr("(1+2)x3", &vars), 9.0);
        assert_eq!(eval_expr("$1/2", &vars), 1.0);
    }

    #[test]
    fn circle_macro_area() {
        let m = MacroDef::new(vec!["1,1,2,0,0".to_string()]);
        let g = m.evaluate(&[], 256);
        let a = fc_geo::area(&g);
        assert!((a - std::f64::consts::PI).abs() < 1e-2, "macro circle area {a}");
    }

    #[test]
    fn macro_with_modifier() {
        // circle whose diameter comes from $1
        let m = MacroDef::new(vec!["1,1,$1,0,0".to_string()]);
        let g = m.evaluate(&[2.0], 256);
        let a = fc_geo::area(&g);
        assert!((a - std::f64::consts::PI).abs() < 1e-2, "param circle area {a}");
    }
}
