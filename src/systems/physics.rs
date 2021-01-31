use cgmath::{prelude::*, Vector2};

use legion::systems::{Builder, CommandBuffer};
use legion::world::{ComponentError, EntityAccessError, EntryRef, Event, EventSender, SubWorld};
use legion::*;

use crossbeam_channel::Receiver;

use nalgebra::Isometry2;

use nphysics2d::force_generator::DefaultForceGeneratorSet;
use nphysics2d::joint::DefaultJointConstraintSet;
use nphysics2d::object::{
    Body, BodyPartHandle, BodyStatus, ColliderDesc, DefaultBodySet, DefaultColliderSet,
    RigidBodyDesc,
};
use nphysics2d::world::{DefaultGeometricalWorld, DefaultMechanicalWorld};

use crate::components::*;
use legion::storage::ArchetypeIndex;
use ncollide2d::shape::ShapeHandle;
use nphysics2d::ncollide2d::shape::{Ball, Cuboid};

pub(crate) trait PhysicsBuilderExtender {
    fn add_physics_systems(&mut self, world: &mut World, resources: &mut Resources) -> &mut Self;
}

impl PhysicsBuilderExtender for Builder {
    fn add_physics_systems(&mut self, world: &mut World, resources: &mut Resources) -> &mut Self {
        resources.insert(PhysicsResource::default());
        return self
            .add_system(validate_physics_entities_system())
            .add_system(make_body_handles_system())
            .add_system(remove_body_handles_system())
            .add_system(flush_command_buffer_system())
            .add_system(make_collider_handles_system())
            .add_system(remove_collider_handles_system())
            .add_system(flush_command_buffer_system())
            .add_system(garbage_system())
            .add_system(entity_world_to_physics_world_system())
            .add_system(step_physics_world_system())
            .add_system(physics_world_to_entity_world_system());
        //      .add_system(movement_system());
    }
}

struct PhysicsResource {
    mechanical_world: DefaultMechanicalWorld<f32>,
    geometrical_world: DefaultGeometricalWorld<f32>,
    bodies: DefaultBodySet<f32>,
    colliders: DefaultColliderSet<f32>,
    joint_constraints: DefaultJointConstraintSet<f32>,
    force_generators: DefaultForceGeneratorSet<f32>,
}

impl PhysicsResource {
    fn step(&mut self) {
        self.mechanical_world.step(
            &mut self.geometrical_world,
            &mut self.bodies,
            &mut self.colliders,
            &mut self.joint_constraints,
            &mut self.force_generators,
        )
    }
}

impl Default for PhysicsResource {
    fn default() -> Self {
        PhysicsResource {
            mechanical_world: DefaultMechanicalWorld::new(
                nalgebra::zero::<nalgebra::Vector2<f32>>(),
            ),
            geometrical_world: DefaultGeometricalWorld::new(),
            bodies: DefaultBodySet::new(),
            colliders: DefaultColliderSet::new(),
            joint_constraints: DefaultJointConstraintSet::new(),
            force_generators: DefaultForceGeneratorSet::new(),
        }
    }
}

fn n2c(input: nalgebra::Vector2<f32>) -> Vector2<f32> {
    return cgmath::Vector2::new(input.x, input.y);
}

fn c2n(input: cgmath::Vector2<f32>) -> nalgebra::Vector2<f32> { return [input.x, input.y].into(); }

#[system]
#[read_component(Position)]
#[read_component(Velocity)]
#[read_component(Speed)]
#[read_component(Acceleration)]
#[write_component(Force)]
#[read_component(Orientation)]
#[read_component(DynamicBody)]
#[read_component(StaticBody)]
fn validate_physics_entities(world: &mut SubWorld, commands: &mut CommandBuffer) {
    let mut query = <(
        Entity,
        TryRead<Position>,
        TryRead<Velocity>,
        TryRead<Force>,
        TryRead<Orientation>,
        TryRead<StaticBody>,
    )>::query()
    .filter(component::<DynamicBody>());
    for (ent, pos, vel, frc, ori, sta) in query.iter(world) {
        if pos.is_none() {
            panic!("missing Position in DynamicBody");
        } else if vel.is_none() {
            panic!("missing Velocity in DynamicBody");
            // TODO: decide if Force should be in our repertoire
            // } else if frc.is_none() {
            //     commands.add_component(*ent, Force::default());
        } else if ori.is_none() {
            panic!("missing Orientation in DynamicBody");
        } else if sta.is_some() {
            // this is an awfully DynamicBody normative perspective
            panic!("There's a naughty StaticBody that really feels DynamicBody inside.");
        }
    }
    let mut query = <(
        Entity,
        TryRead<Position>,
        TryRead<Velocity>,
        TryRead<Speed>,
        TryRead<Acceleration>,
        TryRead<Force>,
        TryRead<Orientation>,
    )>::query()
    .filter(component::<StaticBody>());
    for (ent, pos, vel, spd, acc, frc, ori) in query.iter(world) {
        if pos.is_none() {
            panic!("missing Position in StaticBody");
        } else if vel.is_some() {
            panic!("StaticBody can't have a Velocity component");
        } else if spd.is_some() {
            panic!("StaticBody can't have a Speed component");
        } else if acc.is_some() {
            panic!("StaticBody can't have a Acceleration component");
        } else if frc.is_some() {
            panic!("StaticBody can't have a Force component");
            // } else if ori.is_none() {
            //     panic!("missing Orientation in StaticBody");
        }
    }
}

#[system(for_each)]
#[filter((component::<DynamicBody>() | component::<StaticBody>() | component::<DisabledBody>()) & !component::<BodyHandle>())]
//#[filter(component::<DynamicBody>() | component::<StaticBody>() | component::<DisabledBody>())]
fn make_body_handles(
    world: &mut SubWorld,
    commands: &mut legion::systems::CommandBuffer,
    #[resource] physics: &mut PhysicsResource,
    entity: &Entity,
    dynamic: Option<&DynamicBody>,
    stat: Option<&StaticBody>,
    disabled: Option<&DisabledBody>,
) {
    let body = if let Some(dyna) = dynamic {
        RigidBodyDesc::<f32>::new()
            .status(BodyStatus::Dynamic)
            .gravity_enabled(false)
            .mass(dyna.mass)
    } else if let Some(_) = stat {
        RigidBodyDesc::<f32>::new().status(BodyStatus::Static)
    } else if let Some(_) = disabled {
        RigidBodyDesc::<f32>::new().status(BodyStatus::Disabled)
    } else {
        unreachable!() // the filter should take care of this
    };
    let handle = BodyHandle(physics.bodies.insert(body.build()));
    commands.add_component(*entity, handle);
}

#[system(for_each)]
#[filter(!component::<DynamicBody>() & !component::<StaticBody>() & !component::<DisabledBody>())]
fn remove_body_handles(
    commands: &mut legion::systems::CommandBuffer,
    #[resource] physics: &mut PhysicsResource,
    entity: &Entity,
    handle: &BodyHandle,
) {
    physics.bodies.remove(handle.0);
    commands.remove_component::<BodyHandle>(*entity);
}

#[system(for_each)]
fn flush_command_buffer(world: &mut World, commands: &mut legion::systems::CommandBuffer) {
    commands.flush(world);
}

#[system(for_each)]
#[filter((component::<CircleCollider>() | component::<SquareCollider>()) & !component::<ColliderHandle>())]
fn make_collider_handles(
    world: &SubWorld,
    commands: &mut legion::systems::CommandBuffer,
    #[resource] physics: &mut PhysicsResource,
    entity: &Entity,
    body_handle: &BodyHandle,
    circle: Option<&CircleCollider>,
    square: Option<&SquareCollider>,
) {
    let shape_handle = if let Some(c) = circle {
        ShapeHandle::new(Ball::new(c.radius))
    } else if let Some(s) = square {
        let side_length = s.side_length / 2.0;
        let sides_vec = nalgebra::Vector2::new(side_length, side_length);
        ShapeHandle::new(Cuboid::new(sides_vec))
    } else {
        unreachable!() // the filter should prevent this
    };
    let mut collider = ColliderDesc::<f32>::new(shape_handle);
    let handle = ColliderHandle(
        physics
            .colliders
            .insert(collider.build(BodyPartHandle(body_handle.0, 0))),
    );
    commands.add_component(*entity, handle);
}

#[system(for_each)]
#[filter(!component::<CircleCollider>() & !component::<SquareCollider>())]
fn remove_collider_handles(
    commands: &mut legion::systems::CommandBuffer,
    #[resource] physics: &mut PhysicsResource,
    entity: &Entity,
    body_handle: &ColliderHandle,
) {
    physics.colliders.remove(body_handle.0);
    commands.remove_component::<ColliderHandle>(*entity);
}

#[system]
#[read_component(BodyHandle)]
#[read_component(Position)]
#[read_component(Velocity)]
#[read_component(Orientation)]
#[read_component(DynamicBody)]
fn entity_world_to_physics_world(world: &SubWorld, #[resource] physics: &mut PhysicsResource) {
    let mut query = <(
        Entity,
        Read<BodyHandle>,
        Read<Position>,
        Read<Velocity>,
        Read<Orientation>,
    )>::query()
    .filter(component::<DynamicBody>());
    for (ent, han, pos, vel, ori) in query.iter(world) {
        if let Some(body) = physics.bodies.rigid_body_mut(han.0) {
            body.set_position(Isometry2::new(c2n(pos.0), cgmath::Rad::from(ori.0).0));
            body.set_linear_velocity(c2n(vel.0));
            // and force?
        }
    }
}

#[system]
fn step_physics_world(#[resource] physics: &mut PhysicsResource) { physics.step(); }

#[system]
#[read_component(BodyHandle)]
#[write_component(Position)]
#[write_component(Velocity)]
#[write_component(Orientation)]
fn physics_world_to_entity_world(
    world: &mut SubWorld,
    commands: &mut CommandBuffer,
    #[resource] physics: &PhysicsResource,
) {
    let mut query = <(
        Read<BodyHandle>,
        TryWrite<Position>,
        TryWrite<Velocity>,
        TryWrite<Orientation>,
    )>::query()
    .filter(component::<DynamicBody>() & maybe_changed::<BodyHandle>());
    for (body, pos, vel, ori) in query.iter_mut(world) {
        if let Some(bod) = physics.bodies.rigid_body(body.0) {
            if let Some(p) = pos {
                p.0 = n2c(bod.position().translation.vector);
            }
            if let Some(v) = vel {
                v.0 = n2c(bod.velocity().linear);
            }
            if let Some(o) = ori {
                o.0 = cgmath::Deg::from(cgmath::Rad(bod.position().rotation.angle()));
            }
        }
    }
}

#[system(for_each)]
fn movement(#[resource] frame_time: &FrameTime, pos: &mut Position, vel: &mut Velocity) {
    if vel.0.x.is_finite() && vel.0.y.is_finite() {
        let v = if (vel.0 * frame_time.0).magnitude() < 0.5 {
            vel.0 * frame_time.0
        } else {
            (vel.0 * frame_time.0).normalize() * 0.5
        };
        pos.0 += v;
    } else {
        // TODO: We need to deal with this somehow
        vel.0 = Vector2::new(0.0, 0.0);
        println!("Velocity Hickup");
    }
}
