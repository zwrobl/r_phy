use bytemuck::{Pod, Zeroable};
use std::{
    array::TryFromSliceError,
    ops::{Add, Index, IndexMut, Mul, Neg, Sub},
};

use super::{Vector2, Vector3, Vector4};

#[cfg(test)]
mod test_matrix_2 {
    use super::{Matrix2, Vector2};

    fn get_matrix_2() -> Matrix2 {
        Matrix2::new(Vector2::new(1.0, 2.0), Vector2::new(0.0, 3.0))
    }

    fn get_matrix_2_transposed() -> Matrix2 {
        Matrix2::new(Vector2::new(1.0, 0.0), Vector2::new(2.0, 3.0))
    }

    #[test]
    fn mul() {
        let m = get_matrix_2();
        assert!(m.approx_equal(m * Matrix2::identity()));
        assert!(m.approx_equal(Matrix2::identity() * m));
    }

    #[test]
    fn trace() {
        let m = get_matrix_2();
        assert_eq!(m.trace(), 4.0);
    }

    #[test]
    fn transpose() {
        let m = get_matrix_2();
        let m_t = get_matrix_2_transposed();
        assert!(m.transpose().approx_equal(m_t))
    }

    #[test]
    fn inverse() {
        let m = get_matrix_2();
        assert!(Matrix2::identity().approx_equal(m * m.inv()));
        assert!(Matrix2::identity().approx_equal(m.inv() * m));
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, Zeroable, Pod)]
pub struct Matrix2 {
    pub i: Vector2,
    pub j: Vector2,
}

impl Neg for Matrix2 {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self::Output {
        Self {
            i: -self.i,
            j: -self.j,
        }
    }
}

impl Add for Matrix2 {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self::Output {
        Self {
            i: self.i + rhs.i,
            j: self.j + rhs.j,
        }
    }
}

impl Sub for Matrix2 {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            i: self.i - rhs.i,
            j: self.j - rhs.j,
        }
    }
}

impl Mul<Matrix2> for f32 {
    type Output = Matrix2;
    #[inline]
    fn mul(self, rhs: Matrix2) -> Self::Output {
        Matrix2 {
            i: self * rhs.i,
            j: self * rhs.j,
        }
    }
}

impl Mul<Vector2> for Matrix2 {
    type Output = Vector2;
    #[inline]
    fn mul(self, rhs: Vector2) -> Self::Output {
        rhs.x * self.i + rhs.y * self.j
    }
}

impl Mul<Matrix2> for Matrix2 {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: Self) -> Self::Output {
        Self {
            i: self * rhs.i,
            j: self * rhs.j,
        }
    }
}

impl Index<usize> for Matrix2 {
    type Output = Vector2;
    #[inline]
    fn index(&self, index: usize) -> &Self::Output {
        debug_assert!(index < 2, "Invalid index {} for Matrix2!", index);
        unsafe {
            (&self.i as *const Vector2)
                .add(index)
                .as_ref()
                .unwrap_unchecked()
        }
    }
}

impl IndexMut<usize> for Matrix2 {
    #[inline]
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        debug_assert!(index < 2, "Invalid index {} for Matrix2!", index);
        unsafe {
            (&mut self.i as *mut Vector2)
                .add(index)
                .as_mut()
                .unwrap_unchecked()
        }
    }
}

impl From<Matrix3> for Matrix2 {
    #[inline]
    fn from(value: Matrix3) -> Self {
        Self {
            i: value.i.into(),
            j: value.j.into(),
        }
    }
}

impl From<Matrix4> for Matrix2 {
    #[inline]
    fn from(value: Matrix4) -> Self {
        Self {
            i: value.i.into(),
            j: value.j.into(),
        }
    }
}

impl Matrix2 {
    #[inline]
    pub fn try_from_le_bytes(bytes: &[u8]) -> Result<Self, TryFromSliceError> {
        Ok(Self {
            i: Vector2::try_from_le_bytes(&bytes[0..8])?,
            j: Vector2::try_from_le_bytes(&bytes[8..16])?,
        })
    }

    #[inline]
    pub fn new(i: Vector2, j: Vector2) -> Self {
        Self { i, j }
    }

    #[inline]
    pub fn identity() -> Self {
        Self {
            i: Vector2::x(),
            j: Vector2::y(),
        }
    }

    #[inline]
    pub fn transpose(self) -> Self {
        Self {
            i: Vector2 {
                x: self.i.x,
                y: self.j.x,
            },
            j: Vector2 {
                x: self.i.y,
                y: self.j.y,
            },
        }
    }

    #[inline]
    pub fn det(self) -> f32 {
        self.i.x * self.j.y - self.i.y * self.j.x
    }

    #[inline]
    pub fn inv(self) -> Self {
        self.det().recip()
            * Self {
                i: Vector2 {
                    x: self.j.y,
                    y: -self.i.y,
                },
                j: Vector2 {
                    x: -self.j.x,
                    y: self.i.x,
                },
            }
    }

    #[inline]
    pub fn trace(self) -> f32 {
        self.i.x + self.j.y
    }

    #[inline]
    pub fn is_valid(self) -> bool {
        self.i.is_valid() && self.j.is_valid()
    }

    #[inline]
    pub fn approx_equal(self, rhs: Self) -> bool {
        self.i.approx_equal(rhs.i) && self.j.approx_equal(rhs.j)
    }
}

#[cfg(test)]
mod test_matrix_3 {
    use crate::types::EPS;

    use super::{Matrix3, Vector3};

    fn get_matrix_3() -> Matrix3 {
        Matrix3::new(
            Vector3::new(1.0, 2.0, 3.0),
            Vector3::new(0.0, 4.0, 5.0),
            Vector3::new(0.0, 0.0, 6.0),
        )
    }

    fn get_matrix_3_transposed() -> Matrix3 {
        Matrix3::new(
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(2.0, 4.0, 0.0),
            Vector3::new(3.0, 5.0, 6.0),
        )
    }

    #[test]
    fn mul() {
        let m = get_matrix_3();
        assert!(m.approx_equal(m * Matrix3::identity()));
        assert!(m.approx_equal(Matrix3::identity() * m));
    }

    #[test]
    fn trace() {
        let m = get_matrix_3();
        assert_eq!(m.trace(), 11.0);
    }

    #[test]
    fn transpose() {
        let m = get_matrix_3();
        let m_t = get_matrix_3_transposed();
        assert!(m.transpose().approx_equal(m_t))
    }

    #[test]
    fn inverse() {
        let m = get_matrix_3();
        let m_inv = m.inv();
        assert!(Matrix3::identity().approx_equal(m * m_inv));
        assert!(Matrix3::identity().approx_equal(m_inv * m));
    }

    #[test]
    fn orthonormal() {
        let i = Vector3::new(1.2, 0.42, 0.3);
        let j = Vector3::new(0.2, -0.21, 1.42);
        let k = Vector3::new(-0.2, -2.13, 4.2);
        let m_orth = Matrix3::orthonormal(i, j, k);
        assert!((m_orth.i.length() - 1.0).abs() < EPS);
        assert!((m_orth.j.length() - 1.0).abs() < EPS);
        assert!((m_orth.k.length() - 1.0).abs() < EPS);
        assert!((m_orth.i * m_orth.j).abs() < EPS);
        assert!((m_orth.i * m_orth.k).abs() < EPS);
        assert!((m_orth.k * m_orth.j).abs() < EPS);
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, Zeroable, Pod)]
pub struct Matrix3 {
    pub i: Vector3,
    pub j: Vector3,
    pub k: Vector3,
}

impl Neg for Matrix3 {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self::Output {
        Self {
            i: -self.i,
            j: -self.j,
            k: -self.k,
        }
    }
}

impl Add for Matrix3 {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self::Output {
        Self {
            i: self.i + rhs.i,
            j: self.j + rhs.j,
            k: self.k + rhs.k,
        }
    }
}

impl Sub for Matrix3 {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            i: self.i - rhs.i,
            j: self.j - rhs.j,
            k: self.k - rhs.k,
        }
    }
}

impl Mul<Matrix3> for f32 {
    type Output = Matrix3;
    #[inline]
    fn mul(self, rhs: Matrix3) -> Self::Output {
        Matrix3 {
            i: self * rhs.i,
            j: self * rhs.j,
            k: self * rhs.k,
        }
    }
}

impl Mul<Vector3> for Matrix3 {
    type Output = Vector3;
    #[inline]
    fn mul(self, rhs: Vector3) -> Self::Output {
        rhs.x * self.i + rhs.y * self.j + rhs.z * self.k
    }
}

impl Mul<Matrix3> for Matrix3 {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: Self) -> Self::Output {
        Self {
            i: self * rhs.i,
            j: self * rhs.j,
            k: self * rhs.k,
        }
    }
}

impl Index<usize> for Matrix3 {
    type Output = Vector3;
    #[inline]
    fn index(&self, index: usize) -> &Self::Output {
        debug_assert!(index < 3, "Invalid index {} for Matrix3!", index);
        unsafe {
            (&self.i as *const Vector3)
                .add(index)
                .as_ref()
                .unwrap_unchecked()
        }
    }
}

impl IndexMut<usize> for Matrix3 {
    #[inline]
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        debug_assert!(index < 3, "Invalid index {} for Matrix3!", index);
        unsafe {
            (&mut self.i as *mut Vector3)
                .add(index)
                .as_mut()
                .unwrap_unchecked()
        }
    }
}

impl From<Matrix2> for Matrix3 {
    #[inline]
    fn from(value: Matrix2) -> Self {
        Self {
            i: value.i.into(),
            j: value.j.into(),
            k: Vector3::z(),
        }
    }
}

impl From<Matrix4> for Matrix3 {
    #[inline]
    fn from(value: Matrix4) -> Self {
        Self {
            i: value.i.into(),
            j: value.j.into(),
            k: value.k.into(),
        }
    }
}

impl Matrix3 {
    #[inline]
    pub fn try_from_le_bytes(bytes: &[u8]) -> Result<Self, TryFromSliceError> {
        Ok(Self {
            i: Vector3::try_from_le_bytes(&bytes[0..12])?,
            j: Vector3::try_from_le_bytes(&bytes[12..24])?,
            k: Vector3::try_from_le_bytes(&bytes[24..36])?,
        })
    }

    #[inline]
    pub fn new(i: Vector3, j: Vector3, k: Vector3) -> Self {
        Self { i, j, k }
    }

    #[inline]
    pub fn identity() -> Self {
        Self {
            i: Vector3::x(),
            j: Vector3::y(),
            k: Vector3::z(),
        }
    }

    #[inline]
    pub fn orthonormal(i: Vector3, j: Vector3, k: Vector3) -> Matrix3 {
        let i_norm = i.norm();
        let j_norm = (j - (j * i_norm) * i_norm).norm();
        let k_norm = (k - (k * i_norm) * i_norm - (k * j_norm) * j_norm).norm();
        Matrix3::new(i_norm, j_norm, k_norm)
    }

    #[inline]
    pub fn transpose(self) -> Self {
        Self {
            i: Vector3 {
                x: self.i.x,
                y: self.j.x,
                z: self.k.x,
            },
            j: Vector3 {
                x: self.i.y,
                y: self.j.y,
                z: self.k.y,
            },
            k: Vector3 {
                x: self.i.z,
                y: self.j.z,
                z: self.k.z,
            },
        }
    }

    #[inline]
    pub fn det(self) -> f32 {
        (self.i.x * self.j.y * self.k.z)
            + (self.j.x * self.k.y * self.i.z)
            + (self.k.x * self.i.y * self.j.z)
            - (self.i.z * self.j.y * self.k.x)
            - (self.j.z * self.k.y * self.i.x)
            - (self.k.z * self.i.y * self.j.x)
    }

    #[inline]
    pub fn inv(self) -> Self {
        self.det().recip() * self.adj()
    }

    #[inline]
    pub fn trace(self) -> f32 {
        self.i.x + self.j.y + self.k.z
    }

    #[inline]
    pub fn is_valid(self) -> bool {
        self.i.is_valid() && self.j.is_valid() && self.k.is_valid()
    }

    #[inline]
    pub fn approx_equal(self, rhs: Self) -> bool {
        self.i.approx_equal(rhs.i) && self.j.approx_equal(rhs.j) && self.k.approx_equal(rhs.k)
    }

    #[inline]
    fn adj(self) -> Self {
        let mut a = Matrix3::default();
        for col in 0..3 {
            for row in 0..3 {
                a[col][row] = (-1.0f32).powi((col + row) as _) * self.minor(col, row).det()
            }
        }
        a.transpose()
    }

    #[inline]
    fn minor(self, col: usize, row: usize) -> Matrix2 {
        let mut m = Matrix2::default();
        let mut k = 0;
        for i in 0..3 {
            if i != col {
                let mut l = 0;
                for j in 0..3 {
                    if j != row {
                        m[k][l] = self[i][j];
                        l += 1;
                    }
                }
                k += 1;
            }
        }
        m
    }

    #[inline]
    pub fn euler(&self) -> Vector3 {
        Vector3::new(
            self.i.x.atan2(self.i.y),
            self.j.x.atan2(self.j.y),
            self.k.x.atan2(self.k.y),
        )
    }
}

#[cfg(test)]
mod test_matrix_4 {
    use super::{Matrix4, Vector4};

    fn get_matrix_4() -> Matrix4 {
        Matrix4::new(
            Vector4::new(1.0, 2.0, 3.0, 4.0),
            Vector4::new(0.0, 5.0, 6.0, 7.0),
            Vector4::new(0.0, 0.0, 8.0, 9.0),
            Vector4::new(0.0, 0.0, 0.0, 10.0),
        )
    }

    fn get_matrix_4_transposed() -> Matrix4 {
        Matrix4::new(
            Vector4::new(1.0, 0.0, 0.0, 0.0),
            Vector4::new(2.0, 5.0, 0.0, 0.0),
            Vector4::new(3.0, 6.0, 8.0, 0.0),
            Vector4::new(4.0, 7.0, 9.0, 10.0),
        )
    }

    #[test]
    fn mul() {
        let m = get_matrix_4();
        assert!(m.approx_equal(m * Matrix4::identity()));
        assert!(m.approx_equal(Matrix4::identity() * m));
    }

    #[test]
    fn trace() {
        let m = get_matrix_4();
        assert_eq!(m.trace(), 24.0);
    }

    #[test]
    fn transpose() {
        let m = get_matrix_4();
        let m_t = get_matrix_4_transposed();
        assert!(m.transpose().approx_equal(m_t))
    }

    #[test]
    fn inverse() {
        let m = get_matrix_4();
        let m_inv = m.inv();
        assert!(Matrix4::identity().approx_equal(m * m_inv));
        assert!(Matrix4::identity().approx_equal(m_inv * m));
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, Zeroable, Pod)]
pub struct Matrix4 {
    pub i: Vector4,
    pub j: Vector4,
    pub k: Vector4,
    pub l: Vector4,
}

impl Neg for Matrix4 {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self::Output {
        Self {
            i: -self.i,
            j: -self.j,
            k: -self.k,
            l: -self.l,
        }
    }
}

impl Add for Matrix4 {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self::Output {
        Self {
            i: self.i + rhs.i,
            j: self.j + rhs.j,
            k: self.k + rhs.k,
            l: self.l + rhs.l,
        }
    }
}

impl Sub for Matrix4 {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            i: self.i - rhs.i,
            j: self.j - rhs.j,
            k: self.k - rhs.k,
            l: self.l - rhs.l,
        }
    }
}

impl Mul<Matrix4> for f32 {
    type Output = Matrix4;
    #[inline]
    fn mul(self, rhs: Matrix4) -> Self::Output {
        Matrix4 {
            i: self * rhs.i,
            j: self * rhs.j,
            k: self * rhs.k,
            l: self * rhs.l,
        }
    }
}

impl Mul<Vector4> for Matrix4 {
    type Output = Vector4;
    #[inline]
    fn mul(self, rhs: Vector4) -> Self::Output {
        rhs.x * self.i + rhs.y * self.j + rhs.z * self.k + rhs.w * self.l
    }
}

impl Mul<Matrix4> for Matrix4 {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: Self) -> Self::Output {
        Self {
            i: self * rhs.i,
            j: self * rhs.j,
            k: self * rhs.k,
            l: self * rhs.l,
        }
    }
}

impl Index<usize> for Matrix4 {
    type Output = Vector4;
    #[inline]
    fn index(&self, index: usize) -> &Self::Output {
        debug_assert!(index < 4, "Invalid index {} for Matrix4!", index);
        unsafe {
            (&self.i as *const Vector4)
                .add(index)
                .as_ref()
                .unwrap_unchecked()
        }
    }
}

impl IndexMut<usize> for Matrix4 {
    #[inline]
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        debug_assert!(index < 4, "Invalid index {} for Matrix4!", index);
        unsafe {
            (&mut self.i as *mut Vector4)
                .add(index)
                .as_mut()
                .unwrap_unchecked()
        }
    }
}

impl From<Matrix2> for Matrix4 {
    #[inline]
    fn from(value: Matrix2) -> Self {
        Self {
            i: value.i.into(),
            j: value.j.into(),
            k: Vector4::z(),
            l: Vector4::w(),
        }
    }
}

impl From<Matrix3> for Matrix4 {
    #[inline]
    fn from(value: Matrix3) -> Self {
        Self {
            i: value.i.into(),
            j: value.j.into(),
            k: value.k.into(),
            l: Vector4::w(),
        }
    }
}

impl Matrix4 {
    #[inline]
    pub fn try_from_le_bytes(bytes: &[u8]) -> Result<Self, TryFromSliceError> {
        Ok(Self {
            i: Vector4::try_from_le_bytes(&bytes[0..16])?,
            j: Vector4::try_from_le_bytes(&bytes[16..32])?,
            k: Vector4::try_from_le_bytes(&bytes[32..48])?,
            l: Vector4::try_from_le_bytes(&bytes[48..64])?,
        })
    }

    #[inline]
    pub fn new(i: Vector4, j: Vector4, k: Vector4, l: Vector4) -> Self {
        Self { i, j, k, l }
    }

    #[inline]
    pub fn identity() -> Self {
        Self {
            i: Vector4::x(),
            j: Vector4::y(),
            k: Vector4::z(),
            l: Vector4::w(),
        }
    }

    #[inline]
    pub fn transpose(self) -> Self {
        Self {
            i: Vector4 {
                x: self.i.x,
                y: self.j.x,
                z: self.k.x,
                w: self.l.x,
            },
            j: Vector4 {
                x: self.i.y,
                y: self.j.y,
                z: self.k.y,
                w: self.l.y,
            },
            k: Vector4 {
                x: self.i.z,
                y: self.j.z,
                z: self.k.z,
                w: self.l.z,
            },
            l: Vector4 {
                x: self.i.w,
                y: self.j.w,
                z: self.k.w,
                w: self.l.w,
            },
        }
    }

    #[inline]
    pub fn det(self) -> f32 {
        (0usize..4)
            .map(|col| (-1.0f32).powi((col + 1) as _) * self[col][3] * self.minor(col, 3).det())
            .sum()
    }

    #[inline]
    pub fn inv(self) -> Self {
        self.det().recip() * self.adj()
    }

    #[inline]
    pub fn trace(self) -> f32 {
        self.i.x + self.j.y + self.k.z + self.l.w
    }

    #[inline]
    pub fn is_valid(self) -> bool {
        self.i.is_valid() && self.j.is_valid() && self.k.is_valid() && self.l.is_valid()
    }

    #[inline]
    pub fn approx_equal(self, rhs: Self) -> bool {
        self.i.approx_equal(rhs.i)
            && self.j.approx_equal(rhs.j)
            && self.k.approx_equal(rhs.k)
            && self.l.approx_equal(rhs.l)
    }

    #[inline]
    fn adj(self) -> Self {
        let mut a = Matrix4::default();
        for col in 0..4 {
            for row in 0..4 {
                a[col][row] = (-1.0f32).powi((col + row) as _) * self.minor(col, row).det()
            }
        }
        a.transpose()
    }

    #[inline]
    fn minor(self, col: usize, row: usize) -> Matrix3 {
        let mut m = Matrix3::default();
        let mut k = 0;
        for i in 0..4 {
            if i != col {
                let mut l = 0;
                for j in 0..4 {
                    if j != row {
                        m[k][l] = self[i][j];
                        l += 1;
                    }
                }
                k += 1;
            }
        }
        m
    }
}
