//! Transformations of the space that a page is drawn in.

/// An affine transformation, as the six numbers of a PDF matrix.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Matrix {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Matrix {
    pub const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    /// The transformation that does `self` first and `outer` after it.
    pub fn then(&self, outer: &Matrix) -> Matrix {
        Matrix {
            a: self.a * outer.a + self.b * outer.c,
            b: self.a * outer.b + self.b * outer.d,
            c: self.c * outer.a + self.d * outer.c,
            d: self.c * outer.b + self.d * outer.d,
            e: self.e * outer.a + self.f * outer.c + outer.e,
            f: self.e * outer.b + self.f * outer.d + outer.f,
        }
    }

    pub fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
    }

    /// Where a step of (`x`, `y`) leads after the transformation, which
    /// does not depend on where the step starts.
    pub fn apply_to_step(&self, x: f64, y: f64) -> (f64, f64) {
        (self.a * x + self.c * y, self.b * x + self.d * y)
    }
}

/// From the six numbers in the order in which a PDF writes them.
impl From<[f64; 6]> for Matrix {
    fn from([a, b, c, d, e, f]: [f64; 6]) -> Self {
        Self { a, b, c, d, e, f }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Doubles lengths and then moves by (10, 20).
    const DOUBLE_AND_MOVE: Matrix = Matrix {
        a: 2.0,
        b: 0.0,
        c: 0.0,
        d: 2.0,
        e: 10.0,
        f: 20.0,
    };

    /// Turns the y axis over.
    const FLIP: Matrix = Matrix {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: -1.0,
        e: 0.0,
        f: 0.0,
    };

    #[test]
    fn identity_leaves_a_point_where_it_is() {
        assert_eq!(Matrix::IDENTITY.apply(3.0, 4.0), (3.0, 4.0));
    }

    #[test]
    fn matrix_scales_before_it_moves() {
        assert_eq!(DOUBLE_AND_MOVE.apply(3.0, 4.0), (16.0, 28.0));
    }

    #[test]
    fn composition_does_the_inner_transformation_first() {
        let flipped_then_doubled = FLIP.then(&DOUBLE_AND_MOVE);
        assert_eq!(flipped_then_doubled.apply(3.0, 4.0), (16.0, 12.0));
        let doubled_then_flipped = DOUBLE_AND_MOVE.then(&FLIP);
        assert_eq!(doubled_then_flipped.apply(3.0, 4.0), (16.0, -28.0));
    }
}
