
use specs::prelude::*;
use std::f32::consts::{FRAC_PI_2};

use cgmath::{prelude::*, Vector2};

use crate::components::*;

pub mod assets;
pub mod physics;
pub mod player;
pub mod rendering;
pub mod world_gen;

pub struct SphericalOffsetSystem;

impl<'a> System<'a> for SphericalOffsetSystem {
    type SystemData = (
        ReadStorage<'a, Position>,
        ReadStorage<'a, SphericalOffset>,
        WriteStorage<'a, Position3D>,
    );

    fn run(&mut self, (pos2d, offset, mut pos3d): Self::SystemData) {
        for (pos2d, follow, pos3d) in (&pos2d, &offset, &mut pos3d).join() {
            pos3d.0 = pos2d.to_vec3();

            pos3d.0.x += follow.radius * follow.theta.cos() * follow.phi.cos();
            pos3d.0.y += follow.radius * follow.theta.sin() * follow.phi.cos();
            pos3d.0.z += follow.radius * follow.phi.sin();
        }
    }
}

pub struct HitPointRegenSystem;

impl<'a> System<'a> for HitPointRegenSystem {
    type SystemData = (
        Entities<'a>,
        ReadExpect<'a, FrameTime>,
        WriteStorage<'a, HitPoints>,
        Read<'a, LazyUpdate>,
    );

    fn run(&mut self, (ents, frame_time, mut hp, updater): Self::SystemData) {
        for (ent, hp) in (&ents, &mut hp).join() {
            if hp.health <= 0.0 {
                updater.remove::<AIFollow>(ent);
                updater.remove::<Destination>(ent);
            } else {
                hp.health += 0.7654321 * frame_time.0;
                hp.health = hp.max.min(hp.health);
            }
        }
    }
}

pub struct AIFollowSystem;

impl<'a> System<'a> for AIFollowSystem {
    type SystemData = (
        Entities<'a>,
        WriteStorage<'a, Destination>,
        WriteStorage<'a, Orientation>,
        ReadStorage<'a, AIFollow>,
        ReadStorage<'a, Position>,
    );

    fn run(&mut self, (ents, mut dest, mut orient, follow, pos): Self::SystemData) {
        for (ent, orient, follow, hunter) in (&ents, (&mut orient).maybe(), &follow, &pos).join() {
            if let Some(hunted) = pos.get(follow.target) {
                let difference: Vector2<f32> = hunted.0 - hunter.0;
                let distance = difference.magnitude();
                if distance > follow.minimum_distance {
                    dest.insert(ent, Destination::simple(hunted.0));
                    if let Some(orientation) = orient {
                        orientation.0 = cgmath::Deg::from(difference.angle(Vector2::unit_y()));
                    }
                }
            }
        }
    }
}

pub struct GoToDestinationSystem;

impl<'a> System<'a> for GoToDestinationSystem {
    type SystemData = (
        Entities<'a>,
        Read<'a, LazyUpdate>,
        ReadExpect<'a, FrameTime>,
        WriteStorage<'a, Destination>,
        ReadStorage<'a, Position>,
        WriteStorage<'a, Velocity>,
        ReadStorage<'a, Speed>,
        ReadStorage<'a, Acceleration>,
    );

    fn run(&mut self, (ents, updater, frame_time, mut dests, pos, mut vel, speed, acc): Self::SystemData) {

        const EPSILON : f32 = 0.05;

        for (ent, dest, hunter, vel, speed, accel) in (&ents, &mut dests, &pos, &mut vel, &speed, &acc).join() {
            // check if straight path is available, line drawing? or just navmesh
            // if not do A* and add intermediate destination component for next node in path
            // or just make Destination an object inheriting from the abstract destinations
            // class.
            let to_dest: Vector2<f32> = dest.goal - hunter.0;

            if to_dest.magnitude() < EPSILON {
                updater.remove::<Destination>(ent);
                vel.0 = Vector2::new(0.0, 0.0);
            } else {
                let direction = to_dest.normalize();
                let time_to_stop = speed.0 / accel.0;
                let slowdown = FRAC_PI_2.min(to_dest.magnitude() / time_to_stop * 0.5).sin();
                let target_velocity = direction * speed.0 * slowdown;
                let delta: Vector2<f32> = target_velocity - vel.0;
                let velocity_change = (accel.0 * frame_time.0).min(delta.magnitude());

                if delta != Vector2::unit_x() * 0.0 {
                    vel.0 += delta.normalize() * velocity_change;
                }
            }
        }
    }
}

pub struct IntermediateDestinationSystem;

impl<'a> System<'a> for IntermediateDestinationSystem {
    type SystemData = (

    );

    fn run(&mut self, (): Self::SystemData) {

    }
}
