use bevy::{
    core::FixedTimestep,
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    prelude::*,
};
use rand::prelude::*;

//============================================================================================================================================

const PHYSICS_TIME_STEP: f32 = 1. / 60.;
const ANIMATION_TIME_STEP: f32 = 1. / 8.;

const INIT_FLOCK_SIZE: u32 = 300;
const SCALE: f32 = 2.5;

const PADDING: f32 = 500.;

//============================================================================================================================================

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugin(LogDiagnosticsPlugin::default())
        .add_plugin(FrameTimeDiagnosticsPlugin::default())
        .insert_resource(ClearColor(Color::rgb(1.0, 1.0, 1.0)))
        .add_startup_system(setup)
        .add_startup_system(add_flock)
        .add_system_set(
            SystemSet::new()
                .with_run_criteria(FixedTimestep::step(PHYSICS_TIME_STEP as f64))
                .with_system(flocking)
                .with_system(movement)
                .with_system(wrapping)
        )
        .add_system_set(
            SystemSet::new()
                .with_run_criteria(FixedTimestep::step(ANIMATION_TIME_STEP as f64))
                .with_system(sprite_animation)
                .with_system(sprite_x_direction)
                .with_system(sprite_z_layer)
        )
        .run();
}

//============================================================================================================================================

#[derive(Component, Deref, DerefMut)]
struct Velocity(Vec2);

#[derive(Component)]
struct Boid;

#[derive(Component, Clone)]
struct Flock {
    alignment_strength: f32,
    cohesion_strength: f32,
    avoidance_strength: f32,
    speed: f32,
    accel: f32,
    radius: f32,
}

//============================================================================================================================================

fn setup(mut commands: Commands) {
    let mut camera = OrthographicCameraBundle::new_2d();
    camera.orthographic_projection.scale = SCALE;
    commands.spawn_bundle(camera);
}

fn add_flock(windows: Res<Windows>, mut commands: Commands, asset_server: Res<AssetServer>, mut texture_atlases: ResMut<Assets<TextureAtlas>>) {
    let mut rng = rand::thread_rng();

    let bounds_x: f32 = windows.get_primary().unwrap().width() * SCALE / 2.;
    let bounds_y: f32 = windows.get_primary().unwrap().height() * SCALE / 2.;

    let texture_handle = asset_server.load("sprites/bird.png");
    let texture_atlas = TextureAtlas::from_grid(texture_handle, Vec2::new(200.0, 200.0), 6, 1);
    let texture_atlas_handle = texture_atlases.add(texture_atlas);

    commands.spawn()
        .insert(Flock {
            alignment_strength: 2.,
            cohesion_strength: 1.,
            avoidance_strength: 1.2,
            speed: 130.,
            accel: 40.,
            radius: 60.
        })
        .with_children(|flock| {
            for _ in 1..=INIT_FLOCK_SIZE {
                flock.spawn()
                    .insert(Boid)
                    .insert(Velocity(Vec2::new(rng.gen_range(-100f32..=100f32), rng.gen_range(-100f32..=100f32)).into()))
                    .insert_bundle(SpriteSheetBundle {
                        global_transform: GlobalTransform::from_translation(Vec3::new(rng.gen_range(-bounds_x..=bounds_x), rng.gen_range(-bounds_y..=bounds_y), 1.)),
                        texture_atlas: texture_atlas_handle.clone(),
                        ..default()
                    });
            }
        });
}

fn movement(mut query: Query<(&mut GlobalTransform, &Velocity)>) {
    for (mut transform, velocity) in query.iter_mut() {
        transform.translation += (velocity.0 * PHYSICS_TIME_STEP).extend(0.0);
    }
}

fn flocking(query: Query<(&Flock, &Children)>, mut child_query: Query<(&mut Velocity, &GlobalTransform), With<Boid>>) {
    for (flock, children) in query.iter() {
        let mut boids = Vec::new();

        for child in children.iter() {
            if let Ok((velocity, transform)) = child_query.get(*child) {
                boids.push((child.id(), velocity.0, transform.translation.truncate()));
            }
        }

        for child in children.iter() {
            if let Ok((mut velocity, transform)) = child_query.get_mut(*child) {

                let mut acceleration: Vec2 = calculate_behaviour(child.id(), velocity.0, transform.translation.truncate(), &boids, &flock) * flock.speed;

                if acceleration.length_squared() > flock.accel * flock.accel {
                    acceleration = acceleration.normalize() * flock.accel;
                }

                velocity.0 += acceleration;

                if velocity.0.length_squared() > flock.speed * flock.speed {
                    velocity.0 = velocity.0.normalize() * flock.speed;
                }
            }
        }
    }
}

fn calculate_behaviour(id: u32, velocity:Vec2, position: Vec2, boids: &[(u32, Vec2, Vec2)], flock: &Flock) -> Vec2 {
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

        if offset_squared >= flock.radius * flock.radius {
            continue;
        }
        n_neighbors += 1.;

        avoidance += offset;
        alignment += *other_velocity;
        cohesion += *other_position;
    }
    if n_neighbors == 0. {return velocity}

    cohesion -= position;

    alignment *= flock.alignment_strength;
    cohesion *= flock.cohesion_strength;
    avoidance *= flock.avoidance_strength;

    alignment /= n_neighbors;
    cohesion /= n_neighbors;
    avoidance /= n_neighbors;

    if alignment.length_squared() > flock.alignment_strength * flock.alignment_strength {
        alignment = alignment.normalize();
        alignment *= flock.alignment_strength;
    }
    if cohesion.length_squared() > flock.cohesion_strength * flock.cohesion_strength {
        cohesion = cohesion.normalize();
        cohesion *= flock.cohesion_strength;
    }
    if avoidance.length_squared() > flock.avoidance_strength * flock.avoidance_strength {
        avoidance = avoidance.normalize();
        avoidance *= flock.avoidance_strength;
    }

    return alignment + cohesion + avoidance;
}

fn wrapping(windows: Res<Windows>, mut query: Query<&mut GlobalTransform, With<Boid>>) {
    let bounds_x: f32 = windows.get_primary().unwrap().width() * SCALE / 2.;
    let bounds_y: f32 = windows.get_primary().unwrap().height() * SCALE / 2.;

    for mut transform in query.iter_mut() {
        if transform.translation.x > bounds_x+PADDING {transform.translation.x = -bounds_x;}
        else if transform.translation.x < -bounds_x-PADDING {transform.translation.x = bounds_x;}
        if transform.translation.y > bounds_y+PADDING {transform.translation.y = -bounds_y;}
        else if transform.translation.y < -bounds_y-PADDING {transform.translation.y = bounds_y;}
    }
}

fn sprite_x_direction(mut query: Query<(&mut TextureAtlasSprite, &Velocity)>) {
    for (mut sprite, velocity) in query.iter_mut() {
        if velocity.x > 0. {sprite.flip_x = true}
        if velocity.x < 0. {sprite.flip_x = false}
    }
}

fn sprite_z_layer(windows: Res<Windows>, mut query: Query<&mut GlobalTransform, With<TextureAtlasSprite>>) {
    for mut transform in query.iter_mut() {
        transform.translation.z = (-transform.translation.y + (windows.get_primary().unwrap().height() * SCALE / 2.)) / 100.;
    }
}

fn sprite_animation(texture_atlases: Res<Assets<TextureAtlas>>, mut query: Query<(&mut TextureAtlasSprite, &Handle<TextureAtlas>)>) {
    for (mut sprite, texture_atlas_handle) in query.iter_mut() {
        let texture_atlas = texture_atlases.get(texture_atlas_handle).unwrap();
        sprite.index = (sprite.index + 1) % texture_atlas.textures.len();
    }
}

//============================================================================================================================================
