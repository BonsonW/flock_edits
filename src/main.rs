use bevy::{
    core::FixedTimestep,
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    prelude::*,
    tasks::{AsyncComputeTaskPool, logical_core_count},
};
use rand::prelude::*;
use std::collections::HashSet;
use std::sync::{Mutex};

//============================================================================================================================================

const PHYSICS_STEP: f32 = 1. / 24.;
const ANIMATION_STEP: f32 = 1. / 8.;

const INIT_FLOCK_SIZE: u32 = 200;
const INIT_HUNT_SIZE: u32 = 6;

const SCREEN_SCALE: f32 = 2.5;
const SCREEN_PADDING: f32 = 400.;

//============================================================================================================================================

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugin(LogDiagnosticsPlugin::default())
        .add_plugin(FrameTimeDiagnosticsPlugin::default())
        .insert_resource(ClearColor(Color::rgb(1.0, 1.0, 1.0)))
        .insert_resource(
            FlockParams {
                alignment_strength: 4.,
                cohesion_strength: 1.,
                avoidance_strength: 1.2,
                speed: 130.,
                radius: 60.
        })
        .insert_resource(
            HuntParams {
                radius: 60.,
                hunt_strength: 100.,
        })
        .add_startup_system(setup)
        .add_system_set(
            SystemSet::new()
                .with_run_criteria(FixedTimestep::step(PHYSICS_STEP as f64))
                .with_system(flocking)
                .with_system(movement)
                .with_system(wrapping)
                .with_system(hunting.before(flocking))
        )
        .add_system_set(
            SystemSet::new()
                .with_run_criteria(FixedTimestep::step(ANIMATION_STEP as f64))
                .with_system(sprite_animation)
                .with_system(sprite_x_direction)
                .with_system(sprite_z_layer.after(movement))
        )
        .run();
}

//============================================================================================================================================

#[derive(Component)]
struct Bird;

#[derive(Component)]
struct Cat;

#[derive(Component, Deref, DerefMut)]
struct Velocity(Vec2);

struct HuntParams {
    hunt_strength: f32,
    radius: f32,
}

struct FlockParams {
    alignment_strength: f32,
    cohesion_strength: f32,
    avoidance_strength: f32,
    speed: f32,
    radius: f32,
}

//============================================================================================================================================

fn setup(windows: Res<Windows>, mut commands: Commands, asset_server: Res<AssetServer>, mut texture_atlases: ResMut<Assets<TextureAtlas>>) {
    let mut rng = rand::thread_rng();

    let bounds_x: f32 = windows.get_primary().unwrap().width() * SCREEN_SCALE / 2.;
    let bounds_y: f32 = windows.get_primary().unwrap().height() * SCREEN_SCALE / 2.;

    // add camera
    let mut camera = OrthographicCameraBundle::new_2d();
    camera.orthographic_projection.scale = SCREEN_SCALE;
    commands.spawn_bundle(camera);

    // add birds
    let texture_handle = asset_server.load("sprites/bird.png");
    let texture_atlas = TextureAtlas::from_grid(texture_handle, Vec2::new(200.0, 200.0), 6, 1);
    let texture_atlas_handle = texture_atlases.add(texture_atlas);

    for _ in 1..=INIT_FLOCK_SIZE {
        commands.spawn()
            .insert(Bird)
            .insert(Velocity(Vec2::new(rng.gen_range(-1f32..=1f32), rng.gen_range(-1f32..=1f32))))
            .insert_bundle(SpriteSheetBundle {
                transform: Transform::from_translation(Vec3::new(rng.gen_range(-bounds_x..=bounds_x), rng.gen_range(-bounds_y..=bounds_y), 1.)),
                texture_atlas: texture_atlas_handle.clone(),
                ..default()
            });
    }

    // add cats
    let texture_handle = asset_server.load("sprites/cat.png");
    let texture_atlas = TextureAtlas::from_grid(texture_handle, Vec2::new(200.0, 200.0), 6, 1);
    let texture_atlas_handle = texture_atlases.add(texture_atlas);

    for _ in 1..=INIT_HUNT_SIZE {
        commands.spawn()
            .insert(Cat)
            .insert(Velocity(Vec2::new(rng.gen_range(-1f32..=1f32), rng.gen_range(-1f32..=1f32))))
            .insert_bundle(SpriteSheetBundle {
                transform: Transform::from_translation(Vec3::new(rng.gen_range(-bounds_x..=bounds_x), rng.gen_range(-bounds_y..=bounds_y), 1.)),
                texture_atlas: texture_atlas_handle.clone(),
                ..default()
            });
    }
}

fn hunting (mut commands: Commands, mut query: Query<(&mut Velocity, &Transform), With<Cat>>, prey_query: Query<(Entity, &Transform), With<Bird>>, params: Res<HuntParams>, thread_pool: Res<AsyncComputeTaskPool>) {
    if prey_query.is_empty(){
        return;
    }
    let kill_list = Mutex::new(HashSet::new());

    query.par_for_each_mut(&thread_pool, logical_core_count(), |(mut velocity, transform)|{
        let mut closest_dist = i32::MAX;
        let mut closest_offset = Vec2::ZERO;

        for (other, other_transform) in prey_query.iter() {
            let offset = other_transform.translation.truncate() - transform.translation.truncate();
            let dist = offset.length_squared() as i32;

            if dist < closest_dist {
                closest_dist = dist;
                closest_offset = offset;

                if (closest_dist as f32) < params.radius * params.radius {
                    let mut kill_list = kill_list.lock().unwrap();
                    kill_list.insert(other);
                    break;
                }
            }
        }

        velocity.0 += closest_offset.normalize() * params.hunt_strength;
    });

    let kill_list = kill_list.lock().unwrap();
    for prey_entity in kill_list.iter() {
        commands.entity(*prey_entity).despawn();
    }
}

fn flocking(mut query: Query<(Entity, &mut Velocity, &Transform)>, params: Res<FlockParams>, thread_pool: Res<AsyncComputeTaskPool>) {
    let mut boids = Vec::new();
    for (entity, velocity, transform) in query.iter() {
        boids.push((entity.id(), velocity.0, transform.translation.truncate()));
    }

    query.par_for_each_mut(&thread_pool, logical_core_count(), |(entity, mut velocity, transform)| {
        velocity.0 = velocity.0 + calculate_flock_behaviour(entity.id(), velocity.0, transform.translation.truncate(), &boids, &params) * params.speed;

        if velocity.0.length_squared() > params.speed * params.speed {
            velocity.0 = velocity.0.normalize() * params.speed;
        }
    }) ;
}

fn calculate_flock_behaviour(id: u32, velocity:Vec2, position: Vec2, boids: &[(u32, Vec2, Vec2)], params: &FlockParams) -> Vec2 {
    let mut alignment = Vec2::ZERO;
    let mut cohesion = Vec2::ZERO;
    let mut avoidance = Vec2::ZERO;
    let mut n_neighbors = 0.;

    for (other_id, other_velocity, other_position) in boids.iter() {
        if other_id == &id {
            continue;
        }
        let offset: Vec2 = position - *other_position;
        let offset_squared = offset.length_squared();

        if offset_squared >= params.radius * params.radius {
            continue;
        }
        n_neighbors += 1.;

        avoidance += offset;
        alignment += *other_velocity;
        cohesion += *other_position;
    }
    if n_neighbors == 0. {return velocity}

    cohesion -= position;

    alignment *= params.alignment_strength;
    cohesion *= params.cohesion_strength;
    avoidance *= params.avoidance_strength;

    alignment /= n_neighbors;
    cohesion /= n_neighbors;
    avoidance /= n_neighbors;

    if alignment.length_squared() > params.alignment_strength * params.alignment_strength {
        alignment = alignment.normalize();
        alignment *= params.alignment_strength;
    }
    if cohesion.length_squared() > params.cohesion_strength * params.cohesion_strength {
        cohesion = cohesion.normalize();
        cohesion *= params.cohesion_strength;
    }
    if avoidance.length_squared() > params.avoidance_strength * params.avoidance_strength {
        avoidance = avoidance.normalize();
        avoidance *= params.avoidance_strength;
    }

    return alignment + cohesion + avoidance;
}

fn movement(mut query: Query<(&mut Transform, &Velocity)>) {
    for (mut transform, velocity) in query.iter_mut() {
        transform.translation += (velocity.0 * PHYSICS_STEP).extend(0.0);
    }
}

fn wrapping(windows: Res<Windows>, mut query: Query<&mut Transform>) {
    let bounds_x: f32 = windows.get_primary().unwrap().width() * SCREEN_SCALE / 2.;
    let bounds_y: f32 = windows.get_primary().unwrap().height() * SCREEN_SCALE / 2.;

    for mut transform in query.iter_mut() {
        if transform.translation.x > bounds_x+SCREEN_PADDING {transform.translation.x = -bounds_x;}
        else if transform.translation.x < -bounds_x-SCREEN_PADDING {transform.translation.x = bounds_x;}
        if transform.translation.y > bounds_y+SCREEN_PADDING {transform.translation.y = -bounds_y;}
        else if transform.translation.y < -bounds_y-SCREEN_PADDING {transform.translation.y = bounds_y;}
    }
}

fn sprite_x_direction(mut query: Query<(&mut TextureAtlasSprite, &Velocity)>) {
    for (mut sprite, velocity) in query.iter_mut() {
        if velocity.x > 0. {sprite.flip_x = true}
        if velocity.x < 0. {sprite.flip_x = false}
    }
}

fn sprite_z_layer(windows: Res<Windows>, mut query: Query<&mut Transform, With<TextureAtlasSprite>>) {
    for mut transform in query.iter_mut() {
        transform.translation.z = (-transform.translation.y + (windows.get_primary().unwrap().height() * SCREEN_SCALE / 2.)) / 100.;
    }
}

fn sprite_animation(texture_atlases: Res<Assets<TextureAtlas>>, mut query: Query<(&mut TextureAtlasSprite, &Handle<TextureAtlas>)>) {
    for (mut sprite, texture_atlas_handle) in query.iter_mut() {
        let texture_atlas = texture_atlases.get(texture_atlas_handle).unwrap();
        sprite.index = (sprite.index + 1) % texture_atlas.textures.len();
    }
}

//============================================================================================================================================
