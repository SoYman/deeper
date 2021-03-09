use cgmath::{Deg, Matrix4, Quaternion, Rotation3, SquareMatrix, Vector3};

pub struct Position(pub Vector3<f32>);
impl From<&Position> for Vector3<f32> {
    fn from(pos: &Position) -> Self { pos.0 }
}
pub struct Rotation(pub Deg<f32>);
impl From<&Rotation> for Rotation3D {
    fn from(rot: &Rotation) -> Self { Rotation3D(Quaternion::from_angle_z(rot.0)) }
}
pub struct Rotation3D(pub Quaternion<f32>);

pub struct Scale(pub f32);

impl From<&Position> for Matrix4<f32> {
    fn from(pos: &Position) -> Self { Matrix4::from_translation(pos.0) }
}
impl From<&Rotation> for Matrix4<f32> {
    fn from(rot: &Rotation) -> Self { Matrix4::from_angle_z(rot.0) }
}
impl From<&Rotation3D> for Matrix4<f32> {
    fn from(rot: &Rotation3D) -> Self { Matrix4::from(rot.0) }
}
impl From<&Scale> for Matrix4<f32> {
    fn from(scale: &Scale) -> Self { Matrix4::from_scale(scale.0) }
}

#[derive(Copy, Clone)]
pub struct Transform {
    pub absolute: Matrix4<f32>,
    pub relative: Matrix4<f32>,
}

impl Transform {
    pub fn identity() -> Self {
        Transform {
            absolute: Matrix4::identity(),
            relative: Matrix4::identity(),
        }
    }
}
