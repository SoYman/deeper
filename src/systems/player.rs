use std::f32::consts::PI;

use cgmath::num_traits::clamp;
use cgmath::{Deg, InnerSpace, Vector2, Vector3};
use entity_smith::Smith;
use graphics::components::{Camera, Target};
use input::{Command, CommandManager, InputState};
use legion::systems::ParallelRunnable;
use legion::world::SubWorld;
use legion::{EntityStore, IntoQuery, SystemBuilder};
use physics::Velocity;
use transforms::{Position, Rotation, SphericalOffset, Transform};

use crate::components::{Destination, HitPoints, Player, PlayerCamera};
use crate::world_gen::components::Faction;

pub fn camera_control_system() -> impl ParallelRunnable {
    SystemBuilder::new("camera_control_system")
        .write_component::<Camera>()
        .write_component::<SphericalOffset>()
        .write_component::<Destination>()
        .write_component::<Velocity>()
        .read_component::<Position>()
        .read_component::<Target>()
        .read_component::<Transform>()
        .read_resource::<CommandManager>()
        .read_resource::<InputState>()
        .read_resource::<PlayerCamera>()
        .build(move |cmd, world, resources, _| {
            camera_control(world, cmd, &resources.0, &resources.1, &resources.2);
        })
}

pub fn camera_control(
    world: &mut SubWorld,
    _: &mut legion::systems::CommandBuffer,
    command_manager: &CommandManager,
    input: &InputState,
    player_cam: &PlayerCamera,
) {
    // Should these be a feature of the spherical offset?
    const MINIMUM_PHI: f32 = 0.1 * PI;
    const MAXIMUM_PHI: f32 = 0.3 * PI;

    const MINIMUM_RADIUS: f32 = 5.0;
    const MAXIMUM_RADIUS: f32 = 20.0;

    let (mut camera_world, mut world) = world.split::<&mut Camera>();
    let (mut offset_world, mut world) = world.split::<&mut SphericalOffset>();
    let (mut velocity_world, world) = world.split::<&mut Velocity>();

    let mut camera = <&mut Camera>::query()
        .get_mut(&mut camera_world, player_cam.entity)
        .unwrap();

    let mut cam_offset = <&mut SphericalOffset>::query()
        .get_mut(&mut offset_world, player_cam.entity)
        .unwrap();

    // Zoom controls
    cam_offset.radius += -input.mouse.scroll * cam_offset.radius_delta;
    cam_offset.radius = clamp(cam_offset.radius, MINIMUM_RADIUS, MAXIMUM_RADIUS);

    cam_offset.phi = (cam_offset.radius - MINIMUM_RADIUS) / (MAXIMUM_RADIUS - MINIMUM_RADIUS)
        * (MAXIMUM_PHI - MINIMUM_PHI)
        + MINIMUM_PHI;

    // camera orbiting system enabled for now
    if command_manager.get(Command::PlayerOrbitCamera) {
        let mouse_delta = input.mouse.delta();
        cam_offset.theta += cam_offset.theta_delta * mouse_delta.x;
    }

    if let Ok(cam_target_pos) = <&transforms::Transform>::query()
        .get(
            &world,
            <&Target>::query()
                .get(&world, player_cam.entity)
                .unwrap()
                .entity,
        )
        .map(|trans| trans.world_position())
    {
        if let Ok(cam_pos) = <&transforms::Transform>::query()
            .get(&world, player_cam.entity)
            .map(|trans| trans.world_position())
        {
            // let (cam_pos, height): (&Position, &Height) = <(&Position, &Height)>::query()
            //     .get(&world, player_cam.entity)
            //     .unwrap();
            // let cam_pos = cam_pos.0.extend(height.0.x);

            let to_center: Vector3<f32> = (cam_target_pos - cam_pos).normalize() * 5.0;
            let cam_front = to_center.truncate();
            let cam_right = Vector2::new(to_center.y, -to_center.x);

            let mut new_velocity = Vector2::new(0.0, 0.0);

            if command_manager.get(Command::PlayerCameraMoveUp) {
                new_velocity += cam_front;
                camera.roaming = true;
            }
            if command_manager.get(Command::PlayerCameraMoveLeft) {
                new_velocity -= cam_right;
                camera.roaming = true;
            }
            if command_manager.get(Command::PlayerCameraMoveDown) {
                new_velocity -= cam_front;
                camera.roaming = true;
            }
            if command_manager.get(Command::PlayerCameraMoveRight) {
                new_velocity += cam_right;
                camera.roaming = true;
            }

            // Need to deal with removing the destination also
            if camera.roaming {
                velocity_world
                    .entry_mut(player_cam.entity)
                    .unwrap()
                    .get_component_mut::<Velocity>()
                    .unwrap()
                    .0 = new_velocity;
            }
        }
    }
}

pub fn player_system() -> impl ParallelRunnable {
    SystemBuilder::new("player_system")
        .write_component::<Rotation>()
        .write_component::<Destination>()
        .write_component::<Camera>()
        .read_component::<Position>()
        .read_component::<Transform>()
        .read_component::<Target>()
        .read_component::<Faction>()
        .read_component::<HitPoints>()
        .read_resource::<InputState>()
        .read_resource::<graphics::GraphicsContext>()
        .read_resource::<Player>()
        .read_resource::<PlayerCamera>()
        .build(move |cmd, world, resources, _| {
            player(
                world,
                cmd,
                &resources.0,
                &resources.1,
                &resources.2,
                &resources.3,
            )
        })
}

pub fn player(
    world: &mut SubWorld,
    commands: &mut legion::systems::CommandBuffer,
    input: &InputState,
    context: &graphics::GraphicsContext,
    player: &Player,
    player_cam: &PlayerCamera,
) {
    // We need to do this to get mutable accesses to multiple components at once.
    // It is possible that we can fix this by creating more systems
    let (mut camera_world, mut world) = world.split::<&mut Camera>();
    let (mut orient_world, world) = world.split::<&mut Rotation>();

    let mouse_pos = input.mouse.pos;

    // Click to move around
    // Note(Jökull): We need to make this prettier
    if input.mouse.left.down {
        // TODO: Clean up

        let mut camera: &mut Camera = <&mut Camera>::query()
            .get_mut(&mut camera_world, player_cam.entity)
            .unwrap_or_else(|_| (unreachable!()));

        let camera_position = <&transforms::Transform>::query()
            .get(&world, player_cam.entity)
            .map(|trans| trans.world_position())
            .unwrap_or_else(|_| (unreachable!()));

        let camera_target_pos = <&transforms::Transform>::query()
            .get(
                &world,
                <&Target>::query()
                    .get(&world, player_cam.entity)
                    .unwrap()
                    .entity,
            )
            .map(|trans| trans.world_position())
            .unwrap();

        if let Some(mouse_world_pos) =
            context.screen_to_world(mouse_pos, camera, camera_position, camera_target_pos)
        {
            let ray_delta: Vector3<f32> = mouse_world_pos - camera_position;
            let t: f32 = mouse_world_pos.z / ray_delta.z;
            let ray_hit = (mouse_world_pos - ray_delta * t).truncate();

            commands
                .forge(player.player)
                .any(Destination::simple(ray_hit));
            camera.roaming = false;

            let difference: Vector2<f32> = {
                let player_pos = <&Transform>::query()
                    .get(&world, player.player)
                    .map(|trans| trans.world_position())
                    .expect("I have no place in this world.");
                ray_hit - player_pos.truncate()
            };

            let mut new_rotation = (difference.y / difference.x).atan() / PI * 180.0;
            if difference.x > 0.0 {
                new_rotation += 180.0;
            }
            {
                let player_orient = <&mut Rotation>::query()
                    .get_mut(&mut orient_world, player.model)
                    .expect("We have no direction in life.");
                *player_orient = Deg(new_rotation).into();
            }
        }
    }

    //if input.is_key_pressed(Key::Space) {
    //    for (ent, pos, &HitPoints { max, health }, &faction, dynamic) in (&ents, &pos, &hp, &faction, &dynamic).join() {
    //        let forward_vector = cgmath::Basis2::<f32>::from_angle(player_orient.0).rotate_vector(-Vector2::unit_x());
    //        let in_front = (pos.0 - player_pos.0).normalize().dot(forward_vector.normalize()) > 0.5;
    //        if faction == Faction::Enemies && pos.0.distance(player_pos.0) < 2.0 && in_front {
    //            updater.insert(ent, HitPoints { max, health: (health - 1.0).max(0.0) });
    //            updater.insert(ent, Velocity((pos.0 - player_pos.0).normalize() * 1.5 / dynamic.0));
    //        }
    //    }
    //}
}
