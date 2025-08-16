pub mod projection;

use bytemuck::{Pod, Zeroable};
use std::ops::Mul;

use super::types::{Matrix3, Matrix4, Quat, Vector3, Vector4};

#[cfg(test)]
mod test_transform {
    use crate::types::{Matrix4, Vector3, Vector4};

    use super::Transform;

    fn get_transform() -> Transform {
        Transform::identity()
            .rotate(Vector3::z(), std::f32::consts::FRAC_PI_2)
            .translate(Vector3::z())
            .rotate(Vector3::x(), std::f32::consts::FRAC_PI_4)
            .translate(Vector3::y())
    }

    fn get_transforms() -> (Transform, Transform) {
        (
            Transform::identity()
                .rotate(Vector3::z(), std::f32::consts::FRAC_PI_2)
                .translate(Vector3::z()),
            Transform::identity()
                .rotate(Vector3::x(), std::f32::consts::FRAC_PI_4)
                .translate(Vector3::y()),
        )
    }

    fn get_matrix() -> Matrix4 {
        Matrix4::translate(Vector3::y())
            * Matrix4::rotate_x(std::f32::consts::FRAC_PI_4)
            * Matrix4::translate(Vector3::z())
            * Matrix4::rotate_z(std::f32::consts::FRAC_PI_2)
    }

    #[test]
    fn transform_vector() {
        let t = get_transform();
        let p = t * Vector3::x();
        assert!(p.approx_equal(Vector3::new(0.0, 1.0, 2.0f32.sqrt())));
    }

    #[test]
    fn into_matrix() {
        let t: Matrix4 = get_transform().into();
        let p = t * Vector4::point(Vector3::x());
        assert!(p.approx_equal(Vector4::point(Vector3::new(0.0, 1.0, 2.0f32.sqrt()))));
    }

    #[test]
    fn compose_transform() {
        let (t_a, t_b) = get_transforms();
        let p = (t_a * t_b) * Vector3::x();
        assert!(p.approx_equal(Vector3::new(0.0, 1.0, 2.0f32.sqrt())));
    }

    #[test]
    fn from_matrix() {
        let m = get_matrix();
        let t: Transform = m.into();
        let p_m = m * Vector4::point(Vector3::x());
        let p_t = t * Vector3::x();
        assert!(p_t.approx_equal(Vector3::new(p_m.x, p_m.y, p_m.z)));
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, Zeroable, Pod)]
pub struct Transform {
    pub q: Quat,
    pub t: Vector3,
}

impl From<Transform> for Matrix4 {
    #[inline]
    fn from(value: Transform) -> Self {
        let m: Matrix3 = <Quat as Into<Matrix3>>::into(value.q);
        Matrix4 {
            i: Vector4::vector(m.i),
            j: Vector4::vector(m.j),
            k: Vector4::vector(m.k),
            l: Vector4::point(value.t),
        }
    }
}

impl From<Matrix4> for Transform {
    #[inline]
    fn from(value: Matrix4) -> Self {
        debug_assert!(
            value.i.w == 0.0 && value.j.w == 0.0 && value.k.w == 0.0 && value.l.w == 1.0,
            "Matrix4 is not valid affine transform matrix!"
        );
        let q: Quat = Matrix3::new(
            Vector3::new(value.i.x, value.i.y, value.i.z),
            Vector3::new(value.j.x, value.j.y, value.j.z),
            Vector3::new(value.k.x, value.k.y, value.k.z),
        )
        .into();
        let t = Vector3::new(value.l.x, value.l.y, value.l.z);
        Self { q, t }
    }
}

impl Mul<Vector3> for Transform {
    type Output = Vector3;
    #[inline]
    fn mul(self, rhs: Vector3) -> Self::Output {
        self.q * rhs + self.t
    }
}

impl Mul<Transform> for Transform {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: Transform) -> Self::Output {
        Self {
            q: rhs.q * self.q,
            t: rhs.q * self.t + rhs.t,
        }
    }
}

impl Transform {
    #[inline]
    pub fn new(q: Quat, t: Vector3) -> Self {
        Self { q, t }
    }

    #[inline]
    pub fn identity() -> Self {
        Self {
            q: Quat::identity(),
            t: Vector3::new(0.0, 0.0, 0.0),
        }
    }

    #[inline]
    pub fn rotate(self, axis: Vector3, rad: f32) -> Self {
        let q = Quat::axis_angle(axis, rad);
        Self {
            q: q * self.q,
            t: q * self.t,
        }
    }

    #[inline]
    pub fn rotate_local(self, axis: Vector3, rad: f32) -> Self {
        let q = Quat::axis_angle(axis, rad);
        Self {
            q: q * self.q,
            t: self.t,
        }
    }

    #[inline]
    pub fn translate(self, t: Vector3) -> Self {
        Self {
            q: self.q,
            t: self.t + t,
        }
    }

    #[inline]
    pub fn inv(self) -> Self {
        let q_inv = self.q.inv();
        let t_inv = -(q_inv * self.t);
        Self { q: q_inv, t: t_inv }
    }
}

#[cfg(test)]
mod test_matrix_4_transforms {
    use crate::types::EPS;
    use crate::types::{Matrix4, Vector3, Vector4};

    #[test]
    fn rotate_x() {
        let m = Matrix4::rotate_x(std::f32::consts::FRAC_PI_2);
        let p = m * Vector4::point(Vector3::y());
        assert!(p.approx_equal(Vector4::point(Vector3::z())));
    }

    #[test]
    fn rotate_y() {
        let m = Matrix4::rotate_y(std::f32::consts::FRAC_PI_2);
        let p = m * Vector4::point(Vector3::x());
        assert!(p.approx_equal(Vector4::point(-Vector3::z())));
    }

    #[test]
    fn rotate_z() {
        let m = Matrix4::rotate_z(std::f32::consts::FRAC_PI_2);
        let p = m * Vector4::point(Vector3::x());
        assert!(p.approx_equal(Vector4::point(Vector3::y())));
    }

    #[test]
    fn translate() {
        let m = Matrix4::translate(Vector3::new(1.0, 2.0, 3.0));
        let p = m * Vector4::point(Vector3::new(2.0, 3.0, 1.0));
        assert!(p.approx_equal(Vector4::point(Vector3::new(3.0, 5.0, 4.0))));
    }

    #[test]
    fn scale() {
        let m = Matrix4::scale(4.0);
        let p = m * Vector4::point(Vector3::new(3.0, 2.0, 1.0));
        assert!(p.approx_equal(Vector4::point(Vector3::new(12.0, 8.0, 4.0))));
    }

    #[test]
    fn look_at() {
        let eye = Vector3::new(2.0, 3.0, 4.0);
        let target = Vector3::new(1.0, 1.0, 1.0);
        let m = Matrix4::look_at(eye, target, Vector3::z());
        let p_eye = m * Vector4::point(Vector3::new(2.0, 3.0, 4.0));
        let t = eye - target;
        assert!(p_eye.approx_equal(Vector4::point(Vector3::new(0.0, 0.0, 0.0))));
        assert!((t * Vector3::new(m.i.z, m.j.z, m.k.z) - t.length()).abs() < EPS);
        assert!((Vector3::z() * Vector3::new(m.i.x, m.j.x, m.k.x)).abs() < EPS);
    }
}

impl Matrix4 {
    #[inline]
    pub fn look_at(eye: Vector3, target: Vector3, up: Vector3) -> Matrix4 {
        let f = (eye - target).norm();
        let r = up.cross(f).norm();
        let u = f.cross(r).norm();
        Matrix4 {
            i: Vector4::new(r.x, u.x, f.x, 0.0),
            j: Vector4::new(r.y, u.y, f.y, 0.0),
            k: Vector4::new(r.z, u.z, f.z, 0.0),
            l: Vector4::new(-(eye * r), -(eye * u), -(eye * f), 1.0),
        }
    }

    #[inline]
    pub fn translate(v: Vector3) -> Matrix4 {
        Matrix4::new(
            Vector4::vector(Vector3::x()),
            Vector4::vector(Vector3::y()),
            Vector4::vector(Vector3::z()),
            Vector4::point(v),
        )
    }

    #[inline]
    pub fn rotate_x(rad: f32) -> Matrix4 {
        let cos = rad.cos();
        let sin = rad.sin();
        Matrix3::new(
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.0, cos, sin),
            Vector3::new(0.0, -sin, cos),
        )
        .into()
    }

    #[inline]
    pub fn rotate_y(rad: f32) -> Matrix4 {
        let cos = rad.cos();
        let sin = rad.sin();
        Matrix3::new(
            Vector3::new(cos, 0.0, -sin),
            Vector3::new(0.0, 1.0, 0.0),
            Vector3::new(sin, 0.0, cos),
        )
        .into()
    }

    #[inline]
    pub fn rotate_z(rad: f32) -> Matrix4 {
        let cos = rad.cos();
        let sin = rad.sin();
        Matrix3::new(
            Vector3::new(cos, sin, 0.0),
            Vector3::new(-sin, cos, 0.0),
            Vector3::new(0.0, 0.0, 1.0),
        )
        .into()
    }

    #[inline]
    pub fn scale(s: f32) -> Matrix4 {
        (s * Matrix3::identity()).into()
    }
}
