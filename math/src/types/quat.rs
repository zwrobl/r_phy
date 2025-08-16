use bytemuck::{Pod, Zeroable};
use std::{f32::consts::FRAC_PI_2, ops::Mul};

use super::{Matrix3, Vector3};

#[cfg(test)]
mod test_quat {

    use crate::types::{Matrix3, Matrix4, Quat, Vector3};

    fn get_quat() -> Quat {
        Quat::axis_angle(Vector3::z(), std::f32::consts::FRAC_PI_2)
    }

    fn get_matrix() -> Matrix3 {
        let m = Matrix4::rotate_z(std::f32::consts::FRAC_PI_2);
        Matrix3 {
            i: Vector3::new(m.i.x, m.i.y, m.i.z),
            j: Vector3::new(m.j.x, m.j.y, m.j.z),
            k: Vector3::new(m.k.x, m.k.y, m.k.z),
        }
    }

    #[test]
    fn mul() {
        let quat = get_quat();
        assert!((quat * Vector3::x()).approx_equal(Vector3::y()));
    }

    #[test]
    fn mul_matrix() {
        let quat = get_quat();
        let m = get_matrix();
        let m_q = quat * Matrix3::identity();
        assert!((m_q).approx_equal(m));
    }

    #[test]
    fn inv() {
        let quat_inv = get_quat().inv();
        assert!((quat_inv * Vector3::y()).approx_equal(Vector3::x()));
    }

    #[test]
    fn to_matrix() {
        let m: Matrix3 = get_quat().into();
        let m_inv: Matrix3 = get_quat().inv().into();
        assert!((m * Vector3::x()).approx_equal(Vector3::y()));
        assert!((m_inv * Vector3::y()).approx_equal(Vector3::x()));
    }

    #[test]
    fn from_matrix() {
        let m = get_matrix();
        let q: Quat = m.into();
        let p_m = m * Vector3::x();
        let p_q = q * Vector3::x();
        assert!((p_q).approx_equal(p_m));
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, Zeroable, Pod)]
pub struct Quat {
    pub r: f32,
    pub i: f32,
    pub j: f32,
    pub k: f32,
}

impl Mul<Quat> for f32 {
    type Output = Quat;
    #[inline]
    fn mul(self, rhs: Quat) -> Self::Output {
        Quat {
            r: self * rhs.r,
            i: self * rhs.i,
            j: self * rhs.j,
            k: self * rhs.k,
        }
    }
}

impl Mul<Quat> for Quat {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: Quat) -> Self::Output {
        Self {
            r: self.r * rhs.r - self.i * rhs.i - self.j * rhs.j - self.k * rhs.k,
            i: self.i * rhs.r + self.r * rhs.i + self.j * rhs.k - self.k * rhs.j,
            j: self.j * rhs.r + self.r * rhs.j + self.k * rhs.i - self.i * rhs.k,
            k: self.k * rhs.r + self.r * rhs.k + self.i * rhs.j - self.j * rhs.i,
        }
    }
}

impl Mul<Vector3> for Quat {
    type Output = Vector3;
    #[inline]
    fn mul(self, rhs: Vector3) -> Self::Output {
        let q =
            self * Quat {
                r: 0.0,
                i: rhs.x,
                j: rhs.y,
                k: rhs.z,
            } * self.inv();
        Vector3::new(q.i, q.j, q.k)
    }
}

impl Mul<Matrix3> for Quat {
    type Output = Matrix3;
    fn mul(self, rhs: Matrix3) -> Self::Output {
        Matrix3::new(self * rhs.i, self * rhs.j, self * rhs.k)
    }
}

impl From<Quat> for Matrix3 {
    #[inline]
    fn from(value: Quat) -> Self {
        Matrix3::new(
            value * Vector3::x(),
            value * Vector3::y(),
            value * Vector3::z(),
        )
    }
}

impl From<Matrix3> for Quat {
    #[inline]
    fn from(value: Matrix3) -> Self {
        let t;
        let q;
        if value.k.z < 0.0 {
            if value.i.x > value.j.y {
                t = 1.0 + value.i.x - value.j.y - value.k.z;
                q = Quat::new(
                    value.j.z - value.k.y,
                    t,
                    value.i.y + value.j.x,
                    value.k.x + value.i.z,
                );
            } else {
                t = 1.0 - value.i.x + value.j.y - value.k.z;
                q = Quat::new(
                    value.k.x - value.i.z,
                    value.i.y + value.j.x,
                    t,
                    value.j.z + value.k.y,
                );
            }
        } else if value.i.x < -value.j.y {
            t = 1.0 - value.i.x - value.j.y + value.k.z;
            q = Quat::new(
                value.i.y - value.j.x,
                value.k.x + value.i.z,
                value.j.z + value.k.y,
                t,
            );
        } else {
            t = 1.0 + value.i.x + value.j.y + value.k.z;
            q = Quat::new(
                t,
                value.j.z - value.k.y,
                value.k.x - value.i.z,
                value.i.y - value.j.x,
            );
        }

        (0.5 / t.sqrt()) * q
    }
}

impl Quat {
    #[inline]
    pub fn new(r: f32, i: f32, j: f32, k: f32) -> Self {
        Self { r, i, j, k }
    }

    #[inline]
    pub fn axis_angle(axis: Vector3, rad: f32) -> Self {
        let rad = 0.5 * rad;
        let axis = rad.sin() * axis.norm();
        Self {
            r: rad.cos(),
            i: axis.x,
            j: axis.y,
            k: axis.z,
        }
    }

    #[inline]
    pub fn identity() -> Self {
        Self {
            r: 1.0,
            i: 0.0,
            j: 0.0,
            k: 0.0,
        }
    }

    #[inline]
    pub fn inv(self) -> Self {
        let q = self.mag_squared().recip() * self;
        Self {
            r: q.r,
            i: -q.i,
            j: -q.j,
            k: -q.k,
        }
    }

    #[inline]
    pub fn mag_squared(self) -> f32 {
        self.r * self.r + self.i * self.i + self.j * self.j + self.k * self.k
    }

    #[inline]
    pub fn mag(self) -> f32 {
        self.mag_squared().sqrt()
    }

    #[inline]
    pub fn norm(self) -> Self {
        self.mag().recip() * self
    }

    #[inline]
    pub fn is_valid(self) -> bool {
        self.r.is_finite() && self.i.is_finite() && self.j.is_finite() && self.k.is_finite()
    }

    #[inline]
    pub fn to_euler(self) -> Vector3 {
        let sinr_cosp = 2.0 * (self.r * self.i + self.j * self.k);
        let cosr_cosp = 1.0 - 2.0 * (self.i * self.i + self.j * self.j);
        let roll = sinr_cosp.atan2(cosr_cosp);

        let sinp = (1.0 + 2.0 * (self.r * self.j - self.k * self.i)).sqrt();
        let cosp = (1.0 - 2.0 * (self.r * self.j - self.k * self.i)).sqrt();
        let pitch = 2.0 * sinp.atan2(cosp) - FRAC_PI_2;

        let siny_cosp = 2.0 * (self.r * self.k + self.i * self.j);
        let cosy_cosp = 1.0 - 2.0 * (self.j * self.j + self.k * self.k);
        let yaw = siny_cosp.atan2(cosy_cosp);

        Vector3::new(roll, pitch, yaw)
    }

    #[inline]
    pub fn from_euler(euler: Vector3) -> Self {
        let half_roll = 0.5 * euler.x;
        let half_pitch = 0.5 * euler.y;
        let half_yaw = 0.5 * euler.z;

        let sin_roll = half_roll.sin();
        let cos_roll = half_roll.cos();
        let sin_pitch = half_pitch.sin();
        let cos_pitch = half_pitch.cos();
        let sin_yaw = half_yaw.sin();
        let cos_yaw = half_yaw.cos();

        Self {
            r: cos_roll * cos_pitch * cos_yaw + sin_roll * sin_pitch * sin_yaw,
            i: sin_roll * cos_pitch * cos_yaw - cos_roll * sin_pitch * sin_yaw,
            j: cos_roll * sin_pitch * cos_yaw + sin_roll * cos_pitch * sin_yaw,
            k: cos_roll * cos_pitch * sin_yaw - sin_roll * sin_pitch * cos_yaw,
        }
    }
}
