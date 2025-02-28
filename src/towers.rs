use crate::{creeps::Creep, grid::Grid, level_1::LevelState};
use bevy::prelude::{self, *};
use rand::{
    distributions::Standard,
    prelude::{Distribution, IteratorRandom},
};
use std::time::Duration;
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

mod aquamarine;
mod diamond;

pub struct Plugin;

impl prelude::Plugin for Plugin {
    fn build(&self, app: &mut prelude::AppBuilder) {
        app.add_plugin(diamond::Plugin)
            .add_plugin(aquamarine::Plugin)
            .add_event::<BuildGem>()
            .add_event::<ChooseGem>()
            .add_event::<ProjectileHit>()
            .insert_resource(RangeDisplay::Off)
            .add_system(render_range.system())
            .add_system_set(
                SystemSet::on_update(LevelState::Building).with_system(build_gem.system()),
            )
            .add_system_set(
                SystemSet::on_enter(LevelState::Choosing).with_system(reveal_gems.system()),
            )
            .add_system_set(
                SystemSet::on_update(LevelState::Choosing).with_system(choose_gem.system()),
            )
            .add_system_set(
                SystemSet::on_exit(LevelState::Choosing).with_system(despawn_range_render.system()),
            )
            .add_system(move_projectile.system());
    }
}

pub enum GemQuality {
    Chipped,
}

#[derive(EnumIter)]
pub enum GemType {
    Diamond,
    Aquamarine,
}

impl GemType {
    pub fn color(&self) -> Color {
        match self {
            GemType::Diamond => Color::WHITE,
            GemType::Aquamarine => Color::AQUAMARINE,
        }
    }

    pub fn tower(&self) -> TowerBundle {
        match self {
            GemType::Diamond => TowerBundle {
                damage: Damage(20),
                speed: AttackSpeed(1.2),
                range: Range(10.0),
                cooldown: Cooldown(Timer::from_seconds(1.0, true)),
            },
            GemType::Aquamarine => TowerBundle {
                damage: Damage(10),
                speed: AttackSpeed(0.8),
                range: Range(8.0),
                cooldown: Cooldown(Timer::from_seconds(1.0, true)),
            },
        }
    }
}

impl Distribution<GemType> for Standard {
    fn sample<R: rand::Rng + ?Sized>(&self, rng: &mut R) -> GemType {
        GemType::iter().choose(rng).unwrap()
    }
}

pub struct Gem {
    pub quality: GemQuality,
    pub r#type: GemType,
}

pub struct JustBuilt;

pub struct BuildGem {
    pub pos: (i32, i32),
}

#[allow(clippy::cast_precision_loss)]
fn build_gem(
    mut commands: Commands,
    mut er: EventReader<BuildGem>,
    mut grid: ResMut<Grid>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    for BuildGem { pos } in er.iter() {
        let positions = [
            *pos,
            (pos.0 + 1, pos.1),
            (pos.0, pos.1 + 1),
            (pos.0 + 1, pos.1 + 1),
        ];

        if !grid.buildable(&positions) {
            continue;
        }

        let entity = commands
            .spawn_bundle(PbrBundle {
                mesh: meshes.add(shape::Cube::new(2.0).into()),
                material: mats.add(Color::BEIGE.into()),
                transform: Transform::from_translation(Vec3::new(pos.0 as f32, 0.5, pos.1 as f32)),
                ..PbrBundle::default()
            })
            .insert(JustBuilt)
            .id();
        grid.add_building(&positions, entity)
            .map_err(|_| info!("Failed to add building to {};{}", pos.0, pos.1))
            .ok();
    }
}

pub struct ChooseGem {
    pub pos: (i32, i32),
}

fn reveal_gems(
    mut commands: Commands,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut gems: Query<(Entity, &mut Handle<StandardMaterial>), With<JustBuilt>>,
) {
    for (entity, mut material) in gems.iter_mut() {
        let r#type: GemType = rand::random();
        *material = mats.add(r#type.color().into());
        commands
            .entity(entity)
            .insert_bundle(r#type.tower())
            .insert_bundle((Gem {
                quality: GemQuality::Chipped,
                r#type,
            },));
    }
}

pub struct Rock;

fn choose_gem(
    mut commands: Commands,
    mut er: EventReader<ChooseGem>,
    grid: ResMut<Grid>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut gems: Query<(Entity, &mut Handle<StandardMaterial>), With<JustBuilt>>,
) {
    for ChooseGem { pos } in er.iter() {
        if let Some(chosen_entity) = grid.get(*pos) {
            if gems.get_mut(chosen_entity).is_err() {
                continue;
            }

            for (entity, mut material) in gems.iter_mut() {
                if entity != chosen_entity {
                    *material = mats.add(Color::DARK_GRAY.into());
                    commands
                        .entity(entity)
                        .remove::<Gem>()
                        .remove_bundle::<TowerBundle>()
                        .insert(Rock);
                }
                commands.entity(entity).remove::<JustBuilt>();
            }
        }
    }
}

#[derive(Clone, Copy)]
pub struct Projectile {
    pub origin: Entity,
    pub target: Entity,
}

pub struct ProjectileHit(pub Projectile);

fn move_projectile(
    mut commands: Commands,
    time: Res<Time>,
    mut ew: EventWriter<ProjectileHit>,
    mut projectile: Query<(Entity, &mut Transform, &Projectile)>,
    positions: Query<&GlobalTransform>,
) {
    for (proj_entity, mut transform, projectile) in projectile.iter_mut() {
        let target = positions.get(projectile.target);
        if let Ok(target) = target {
            let mut direction = target.translation - transform.translation;
            direction = direction.normalize();
            direction *= 10.0 * time.delta_seconds();
            transform.translation += direction;

            if (target.translation.x - transform.translation.x).abs() <= 0.05
                && (target.translation.z - transform.translation.z).abs() <= 0.05
            {
                ew.send(ProjectileHit(*projectile));
                commands.entity(proj_entity).despawn_recursive();
            }
        }
    }
}

pub struct Damage(pub u64);

pub struct AttackSpeed(pub f32);

pub struct Range(pub f32);

pub struct Cooldown(Timer);

#[derive(Bundle)]
pub struct TowerBundle {
    damage: Damage,
    speed: AttackSpeed,
    range: Range,
    cooldown: Cooldown,
}

fn launch_projectile(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    gem_position: &GlobalTransform,
    gem_entity: Entity,
    closest_creep: Entity,
) {
    commands
        .spawn_bundle(PbrBundle {
            mesh: meshes.add(
                shape::Icosphere {
                    radius: 0.1,
                    subdivisions: 5,
                }
                .into(),
            ),
            transform: Transform::from_translation(gem_position.translation),
            ..PbrBundle::default()
        })
        .insert(Projectile {
            origin: gem_entity,
            target: closest_creep,
        });
}

fn ccoldown_is_done(cooldown: &mut Cooldown, speed: f32, time: &Time) -> bool {
    cooldown
        .0
        .set_duration(Duration::from_secs_f32(1.0 * speed));
    cooldown.0.tick(time.delta());
    cooldown.0.just_finished()
}

fn get_closest_creep_within_range(
    creeps: &Query<(Entity, &GlobalTransform), With<Creep>>,
    tower_position: &GlobalTransform,
    range: f32,
) -> Option<Entity> {
    let mut closest = None;
    let mut closest_distance = f32::INFINITY;
    for (creep, position) in creeps.iter() {
        let distance = tower_position
            .translation
            .distance_squared(position.translation);

        if distance < closest_distance {
            closest = Some(creep);
            closest_distance = distance;
        }
    }

    if closest_distance >= range.powf(2.0) {
        return None;
    }

    closest
}

#[allow(dead_code)]
enum RangeDisplay {
    Off,
    On(Color),
}

struct RangeVisualization(Entity);

fn render_range(
    mut commands: Commands,
    display: Res<RangeDisplay>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    towers: Query<(Entity, &GlobalTransform, &Range), Added<Range>>,
) {
    if let RangeDisplay::On(color) = *display {
        let color = Color::rgba(color.r(), color.g(), color.b(), 0.1);
        for (entity, transform, Range(range)) in towers.iter() {
            commands
                .spawn_bundle(PbrBundle {
                    mesh: meshes.add(
                        shape::Icosphere {
                            radius: *range,
                            subdivisions: 30,
                        }
                        .into(),
                    ),
                    material: mats.add(color.into()),
                    visible: Visible {
                        is_transparent: true,
                        ..Visible::default()
                    },
                    transform: Transform::from_translation(transform.translation),
                    ..PbrBundle::default()
                })
                .insert(RangeVisualization(entity));
        }
    }
}

fn despawn_range_render(
    mut commands: Commands,
    removed: Query<Entity, With<Rock>>,
    ranges: Query<(Entity, &RangeVisualization)>,
) {
    for removed_entity in removed.iter() {
        for (entity, range) in ranges.iter() {
            if removed_entity == range.0 {
                commands.entity(entity).despawn_recursive();
            }
        }
    }
}
