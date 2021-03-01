use std::f32::consts::PI;

use cgmath::num_traits::clamp;
use cgmath::prelude::*;
use cgmath::{Point3, Vector2, Vector3, Vector4};
use legion::world::SubWorld;
use legion::*;

use crate::components::*;
use crate::graphics;
use crate::graphics::util::{correction_matrix, project_screen_to_world};
use crate::input::{InputState, Key};
use crate::transform::components::{Position, Position3D, Rotation};

#[system]
#[write_component(Camera)]
#[write_component(SphericalOffset)]
#[write_component(Destination)]
#[write_component(Velocity)]
#[read_component(Position3D)]
#[read_component(Position)]
pub fn camera_control(
    world: &mut SubWorld,
    commands: &mut legion::systems::CommandBuffer,
    #[resource] input: &InputState,
    #[resource] player: &Player,
    #[resource] player_cam: &PlayerCamera,
) {
    // Should these be a feature of the spherical offset?
    const MINIMUM_PHI: f32 = 0.1 * PI;
    const MAXIMUM_PHI: f32 = 0.3 * PI;

    const MINIMUM_RADIUS: f32 = 5.0;
    const MAXIMUM_RADIUS: f32 = 20.0;

    let (mut camera_world, mut world) = world.split::<&mut Camera>();
    let (mut offset_world, mut world) = world.split::<&mut SphericalOffset>();
    let (mut velocity_world, world) = world.split::<&mut Velocity>();

    let mut player_cam_entity = camera_world.entry_mut(player_cam.entity).unwrap();
    let mut camera = { player_cam_entity.get_component_mut::<Camera>().unwrap() };

    let mut player_cam_entity = offset_world.entry_mut(player_cam.entity).unwrap();
    let mut cam_offset = player_cam_entity
        .get_component_mut::<SphericalOffset>()
        .unwrap();

    // Zoom controls
    cam_offset.radius += -input.mouse.scroll * cam_offset.radius_delta;
    cam_offset.radius = clamp(cam_offset.radius, MINIMUM_RADIUS, MAXIMUM_RADIUS);

    cam_offset.phi = (cam_offset.radius - MINIMUM_RADIUS) / (MAXIMUM_RADIUS - MINIMUM_RADIUS)
        * (MAXIMUM_PHI - MINIMUM_PHI)
        + MINIMUM_PHI;

    // camera orbiting system enabled for now
    if input.mouse.right.down {
        let mouse_delta = input.mouse.delta();
        cam_offset.theta += cam_offset.theta_delta * mouse_delta.x;
    }

    let cam_pos = world
        .entry_ref(player_cam.entity)
        .unwrap()
        .get_component::<Position>()
        .unwrap()
        .0;

    let cam_3d_pos = world
        .entry_ref(player_cam.entity)
        .unwrap()
        .get_component::<Position3D>()
        .unwrap()
        .0;

    let to_center = (cam_pos.extend(0.0) - cam_3d_pos).normalize() * 5.0;
    let cam_front = Vector2::new(to_center.x, to_center.y);
    let cam_right = Vector2::new(to_center.y, -to_center.x);

    let mut new_velocity = Vector2::new(0.0, 0.0);

    if input.is_key_down(Key::E) {
        new_velocity += cam_front.clone();
        camera.roaming = true;
    }
    if input.is_key_down(Key::S) {
        new_velocity -= cam_right.clone();
        camera.roaming = true;
    }
    if input.is_key_down(Key::D) {
        new_velocity -= cam_front.clone();
        camera.roaming = true;
    }
    if input.is_key_down(Key::F) {
        new_velocity += cam_right.clone();
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
        commands.remove_component::<Destination>(player_cam.entity)
    } else {
        let player_pos = world
            .entry_ref(player.entity)
            .unwrap()
            .get_component::<Position>()
            .unwrap()
            .0;

        commands.add_component::<Destination>(player_cam.entity, Destination::simple(player_pos));
    }
}

#[system]
#[write_component(Rotation)]
#[write_component(Destination)]
#[write_component(Camera)]
#[read_component(Position)]
#[read_component(Position3D)]
#[read_component(Faction)]
#[read_component(HitPoints)]
pub fn player(
    world: &mut SubWorld,
    commands: &mut legion::systems::CommandBuffer,
    #[resource] input: &InputState,
    #[resource] context: &graphics::Context,
    #[resource] player: &Player,
    #[resource] player_cam: &PlayerCamera,
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

        let mut player_cam_entry = camera_world.entry_mut(player_cam.entity).unwrap();
        let mut camera = player_cam_entry.get_component_mut::<Camera>().unwrap();

        let player_cam_entry = world.entry_ref(player_cam.entity).unwrap();
        let camera_position = player_cam_entry.get_component::<Position3D>().unwrap().0;
        let camera_target = player_cam_entry.get_component::<Position>().unwrap().0;

        let aspect_ratio = context.window_size.width as f32 / context.window_size.height as f32;

        let mx_view = cgmath::Matrix4::look_at_rh(
            Point3::from_vec(camera_position),
            Point3::from_vec(camera_target.extend(0.)),
            Vector3::unit_z(),
        );
        let mx_projection = cgmath::perspective(cgmath::Deg(camera.fov), aspect_ratio, 1.0, 1000.0);

        if let Some(mouse_world_pos) = project_screen_to_world(
            Vector3::new(mouse_pos.x, mouse_pos.y, 1.0),
            correction_matrix() * mx_projection * mx_view,
            Vector4::new(
                0,
                0,
                context.window_size.width as i32,
                context.window_size.height as i32,
            ),
        ) {
            let ray_delta: Vector3<f32> = mouse_world_pos - camera_position;
            let t: f32 = mouse_world_pos.z / ray_delta.z;
            let ray_hit = (mouse_world_pos - ray_delta * t).truncate();

            commands.add_component(player.entity, Destination::simple(ray_hit));
            camera.roaming = false;

            let difference: Vector2<f32> = {
                let player_entry = world.entry_ref(player.entity).unwrap();
                let player_pos = player_entry
                    .get_component::<Position>()
                    .expect("I have no place in this world.")
                    .0;
                ray_hit - player_pos
            };

            let mut new_rotation = (difference.y / difference.x).atan() / PI * 180.0;
            if difference.x > 0.0 {
                new_rotation += 180.0;
            }
            {
                let mut player_entry = orient_world.entry_mut(player.entity).unwrap();
                let mut player_orient = player_entry
                    .get_component_mut::<Rotation>()
                    .expect("We have no direction in life.");
                (player_orient.0).0 = new_rotation;
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
