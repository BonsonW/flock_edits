use bevy::{
    core::FixedTimestep,
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    prelude::*
};
use rand::prelude::*;

//============================================================================================================================================

const PHYSICS_TIME_STEP: f32 = 1. / 60.;
const ANIMATION_TIME_STEP: f32 = 1. / 4.;

const INIT_FLOCK_SIZE: u32 = 400;
const SCALE: f32 = 2.5;

//============================================================================================================================================

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugin(LogDiagnosticsPlugin::default())
        .add_plugin(FrameTimeDiagnosticsPlugin::default())
        .add_startup_system(setup)
        .add_startup_system(add_flock)
        .add_system(wrapping)
        .add_system_set(
            SystemSet::new()
                .with_run_criteria(FixedTimestep::step(PHYSICS_TIME_STEP as f64))
                .with_system(flocking)
                .with_system(movement)
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
    separation_strength: f32,
    max_speed: f32,
    max_accel: f32,
    boid_radius: f32,
    flock_radius: f32
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
            alignment_strength: 3.,
            cohesion_strength: 3.,
            separation_strength: 2.,
            max_speed: 150.,
            max_accel: 50.,
            boid_radius: 100.,
            flock_radius: 1000.
        })
        .with_children(|flock| {
            for _ in 1..=INIT_FLOCK_SIZE {
                flock.spawn()
                    .insert(Boid)
                    .insert(Velocity(Vec2::new(rng.gen_range(-2f32..=2f32), rng.gen_range(-2f32..=2f32)).into()))
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
        let old_position = transform.translation;
        transform.translation += (velocity.0 * PHYSICS_TIME_STEP).extend(0.0);
    }
}

fn flocking(query: Query<(&Flock, &Children)>, mut child_query: Query<(&mut Velocity, &GlobalTransform), With<Boid>>) {
    for (flock, children) in query.iter() {
        let mut average_position = Vec2::ZERO;
        let mut average_forward = Vec2::ZERO;
        let mut boids = Vec::new();

        for child in children.iter() {
            if let Ok((velocity, transform)) = child_query.get_mut(*child) {
                average_position += transform.translation.truncate();
                average_forward += velocity.0;
                boids.push((child.id(), transform.translation.truncate()));
            }
        }

        if boids.len() < 1 {
            return
        };

        average_position /= boids.len() as f32;
        average_forward /= boids.len() as f32;

        for (_, mut position) in boids.iter_mut() {
            position.clone_from(&average_position);
        }

        for child in children.iter() {
            if let Ok((mut velocity, transform)) = child_query.get_mut(*child) {
                let position = transform.translation.truncate();

                let alignment = flock.alignment_strength * calculate_alignment(flock.max_speed, average_forward);
                let cohesion = flock.cohesion_strength * calculate_cohesion(position, average_position, flock.flock_radius);
                let separation = flock.separation_strength * calculate_separation(child.id(), flock.boid_radius, position, &boids);

                let mut acceleration: Vec2 = flock.max_speed * (alignment + cohesion + separation);

                if acceleration.length_squared() > flock.max_accel * flock.max_accel {
                    acceleration = acceleration.normalize() * flock.max_accel;
                }

                velocity.0 += acceleration * PHYSICS_TIME_STEP;

                if velocity.0.length_squared() > flock.max_speed + flock.max_speed {
                    velocity.0 = velocity.0.normalize() * flock.max_speed;
                }
            }
        }
    }
}

fn calculate_alignment(max_speed: f32, average_forward: Vec2) -> Vec2 {
    let mut alignment: Vec2  = average_forward / max_speed;

    if alignment.length_squared() > 1.0 {
        alignment = alignment.normalize();
    }

    return alignment
}

fn calculate_cohesion(position: Vec2, average_position: Vec2, flock_radius: f32) -> Vec2 {
    let mut cohesion: Vec2 = average_position - position;

    if cohesion.length_squared() < flock_radius * flock_radius {
        cohesion /= flock_radius;
    } else {
        cohesion = cohesion.normalize();
    }

    return cohesion
}

fn calculate_separation(id: u32, boid_radius: f32, position: Vec2, boids: &[(u32, Vec2)]) -> Vec2 {
    let mut separation = Vec2::ZERO;

    for (other_id, other_position) in boids.into_iter() {
        if other_id != &id {
            let difference: Vec2 = position - *other_position;
            let distance_squared = difference.length_squared();
            let minimum_distance = boid_radius * 2.;

            if distance_squared < minimum_distance * minimum_distance {
                separation += difference.normalize() * (minimum_distance - distance_squared.sqrt()) / minimum_distance;
            }
        }
    }

    if separation.length_squared() > 1.0 {
        separation = separation.normalize();
    }

    return separation
}

fn wrapping(windows: Res<Windows>, mut query: Query<&mut GlobalTransform, With<Boid>>) {
    let bounds_x: f32 = windows.get_primary().unwrap().width() * SCALE / 2.;
    let bounds_y: f32 = windows.get_primary().unwrap().height() * SCALE / 2.;

    for mut transform in query.iter_mut() {
        if transform.translation.x > bounds_x {transform.translation.x = -bounds_x;}
        if transform.translation.x < -bounds_x {transform.translation.x = bounds_x;}
        if transform.translation.y > bounds_y {transform.translation.y = -bounds_y;}
        if transform.translation.y < -bounds_y {transform.translation.y = bounds_y;}
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
