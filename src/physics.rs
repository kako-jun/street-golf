//! Phase 2 のボール物理。
//!
//! [`Course`] の `CornerHeights` から per-tile-type な TriMesh コライダを
//! 生成し、rapier3d の dynamic rigid body でゴルフボールをシミュレーションする。
//! 三角分割は termray の床描画と同じ `(NW, NE, SE) + (NW, SE, SW)` 分割。
//! 垂直軸は Z で、重力は `(0, 0, -9.81)`。
//!
//! 固定タイムステップ 60Hz のアキュムレータで [`Physics::step`] を呼ぶと
//! 実時間経過分だけ `1/60s` ステップを回す。レンダ側のフレームレートと独立。

use rapier3d::prelude::*;

use crate::course::{
    Course, TILE_BUNKER, TILE_FAIRWAY, TILE_GREEN, TILE_ROUGH, TILE_TEE, TILE_WATER,
};
use termray::{HeightMap, TILE_WALL, TileType};

const MAP_W: i32 = 200;
const MAP_H: i32 = 40;
const FIXED_DT: f64 = 1.0 / 60.0;
const BALL_RADIUS: f64 = 0.021;
// R&A/USGA 規格: ゴルフボールは 45.93g 以下。46g を丸めて採用。
const BALL_MASS: f64 = 0.046;
const AT_REST_LINVEL: f64 = 0.05;
const AT_REST_ANGVEL: f64 = 0.1;
const AT_REST_DURATION: f64 = 0.5;
const WALL_CEILING_HALF: f64 = 5.0;
/// `step(dt)` で一度に処理する実時間の上限。スリープ・デバッガ停止等で
/// 巨大な `dt` が来たとき spiral-of-death を防ぐ（最大 15 物理ステップ / 呼び出し）。
const MAX_ACCUMULATED_DT: f64 = 0.25;

/// ボールの現在状態。f64 API で termray 側と揃える。
pub struct BallState {
    pub pos: [f64; 3],
    pub vel: [f64; 3],
    pub at_rest: bool,
}

/// rapier3d の世界を包み込む Phase 2 の物理エンジン。
pub struct Physics {
    gravity: Vector,
    integration_parameters: IntegrationParameters,
    physics_pipeline: PhysicsPipeline,
    island_manager: IslandManager,
    broad_phase: DefaultBroadPhase,
    narrow_phase: NarrowPhase,
    impulse_joints: ImpulseJointSet,
    multibody_joints: MultibodyJointSet,
    ccd_solver: CCDSolver,
    bodies: RigidBodySet,
    colliders: ColliderSet,
    ball_handle: RigidBodyHandle,
    at_rest_timer: f64,
    time_accumulator: f64,
}

impl Physics {
    /// コースと初期スポーン位置からワールドを組み立てる。
    pub fn new(course: &Course, ball_spawn: [f64; 3]) -> Self {
        let mut bodies = RigidBodySet::new();
        let mut colliders = ColliderSet::new();

        build_terrain_colliders(course, &mut colliders);

        let ball_body = RigidBodyBuilder::dynamic()
            .translation(Vector::new(
                ball_spawn[0] as f32,
                ball_spawn[1] as f32,
                ball_spawn[2] as f32,
            ))
            .ccd_enabled(true)
            .build();
        let ball_handle = bodies.insert(ball_body);
        let ball_collider = ColliderBuilder::ball(BALL_RADIUS as f32)
            .mass(BALL_MASS as f32)
            .friction(0.3)
            .restitution(0.35)
            .build();
        colliders.insert_with_parent(ball_collider, ball_handle, &mut bodies);

        let integration_parameters = IntegrationParameters {
            dt: FIXED_DT as f32,
            ..IntegrationParameters::default()
        };

        Self {
            gravity: Vector::new(0.0, 0.0, -9.81),
            integration_parameters,
            physics_pipeline: PhysicsPipeline::new(),
            island_manager: IslandManager::new(),
            broad_phase: DefaultBroadPhase::new(),
            narrow_phase: NarrowPhase::new(),
            impulse_joints: ImpulseJointSet::new(),
            multibody_joints: MultibodyJointSet::new(),
            ccd_solver: CCDSolver::new(),
            bodies,
            colliders,
            ball_handle,
            at_rest_timer: 0.0,
            time_accumulator: 0.0,
        }
    }

    /// 実時間 `dt` 秒分ぶん物理を進める。内部では `1/60s` 固定ステップで回す。
    ///
    /// 蓄積した実時間は `MAX_ACCUMULATED_DT` (0.25s) でクランプする。スリープ・
    /// デバッガ停止などで巨大な `dt` が入ってきても、アキュムレータの while
    /// ループが CPU を焼き切る spiral-of-death を起こさない。
    pub fn step(&mut self, dt: f64) {
        self.time_accumulator += dt;
        if self.time_accumulator > MAX_ACCUMULATED_DT {
            self.time_accumulator = MAX_ACCUMULATED_DT;
        }
        while self.time_accumulator >= FIXED_DT {
            self.physics_pipeline.step(
                self.gravity,
                &self.integration_parameters,
                &mut self.island_manager,
                &mut self.broad_phase,
                &mut self.narrow_phase,
                &mut self.bodies,
                &mut self.colliders,
                &mut self.impulse_joints,
                &mut self.multibody_joints,
                &mut self.ccd_solver,
                &(),
                &(),
            );
            self.update_at_rest(FIXED_DT);
            self.time_accumulator -= FIXED_DT;
        }
    }

    fn update_at_rest(&mut self, dt: f64) {
        let body = &self.bodies[self.ball_handle];
        let lv = body.linvel();
        let av = body.angvel();
        let lin_mag = (lv.x * lv.x + lv.y * lv.y + lv.z * lv.z).sqrt() as f64;
        let ang_mag = (av.x * av.x + av.y * av.y + av.z * av.z).sqrt() as f64;
        if lin_mag < AT_REST_LINVEL && ang_mag < AT_REST_ANGVEL {
            self.at_rest_timer += dt;
        } else {
            self.at_rest_timer = 0.0;
        }
    }

    /// ボールの現在状態（位置・速度・停止判定）を返す。
    pub fn ball_state(&self) -> BallState {
        let body = &self.bodies[self.ball_handle];
        let t = body.translation();
        let v = body.linvel();
        BallState {
            pos: [t.x as f64, t.y as f64, t.z as f64],
            vel: [v.x as f64, v.y as f64, v.z as f64],
            at_rest: self.at_rest_timer >= AT_REST_DURATION,
        }
    }

    /// ボールにショット初速を与える。線形速度を m/s で直接セットし、
    /// impulse = J·s のような質量を跨ぐ計算はしない。停止タイマーもリセットする。
    pub fn launch(&mut self, velocity_mps: [f64; 3]) {
        let body = &mut self.bodies[self.ball_handle];
        body.set_linvel(
            Vector::new(
                velocity_mps[0] as f32,
                velocity_mps[1] as f32,
                velocity_mps[2] as f32,
            ),
            true,
        );
        self.at_rest_timer = 0.0;
    }
}

/// タイル種ごとの (friction, restitution) ペア。
fn tile_material(tile: TileType) -> (f32, f32) {
    match tile {
        TILE_TEE => (0.3, 0.15),
        TILE_FAIRWAY => (0.3, 0.2),
        TILE_ROUGH => (0.5, 0.2),
        TILE_BUNKER => (0.9, 0.05),
        TILE_GREEN => (0.1, 0.1),
        // TODO: Phase 3+ でウォーターハザード判定を追加（1打罰 + リプレース）。
        // 現状 Phase 2 では物理的に solid として衝突するだけ。
        TILE_WATER => (0.2, 0.3),
        TILE_WALL => (0.4, 0.3),
        _ => (0.4, 0.2),
    }
}

/// セル `(x, y)` の NW/NE/SE/SW 4 頂点を `(NW, NE, SE) + (NW, SE, SW)` に分割して返す。
fn cell_triangles(course: &Course, x: i32, y: i32) -> [[Vector; 3]; 2] {
    let h = course.cell_heights(x, y);
    let fx = x as f32;
    let fy = y as f32;
    // CornerHeights.floor indexing: [NW, NE, SW, SE]
    let nw = Vector::new(fx, fy, h.floor[0] as f32);
    let ne = Vector::new(fx + 1.0, fy, h.floor[1] as f32);
    let sw = Vector::new(fx, fy + 1.0, h.floor[2] as f32);
    let se = Vector::new(fx + 1.0, fy + 1.0, h.floor[3] as f32);
    [[nw, ne, se], [nw, se, sw]]
}

/// 地形 TriMesh コライダをタイル種ごとにグループ化して一括生成する。
/// `TILE_WALL` は trimesh には含めず、垂直 cuboid で独立に扱う。
/// （壁セルの z=0 床三角形は垂直 cuboid の陰に隠れて到達不能なため冗長）。
fn build_terrain_colliders(course: &Course, colliders: &mut ColliderSet) {
    let tile_types: [TileType; 6] = [
        TILE_TEE,
        TILE_FAIRWAY,
        TILE_ROUGH,
        TILE_BUNKER,
        TILE_GREEN,
        TILE_WATER,
    ];

    for &target in &tile_types {
        let mut vertices: Vec<Vector> = Vec::new();
        let mut indices: Vec<[u32; 3]> = Vec::new();
        for y in 0..MAP_H {
            for x in 0..MAP_W {
                if course.tile_at(x, y) != Some(target) {
                    continue;
                }
                let tris = cell_triangles(course, x, y);
                for tri in tris {
                    let base = vertices.len() as u32;
                    vertices.push(tri[0]);
                    vertices.push(tri[1]);
                    vertices.push(tri[2]);
                    indices.push([base, base + 1, base + 2]);
                }
            }
        }
        if indices.is_empty() {
            continue;
        }
        let (friction, restitution) = tile_material(target);
        let collider = ColliderBuilder::trimesh(vertices, indices)
            .expect("trimesh build")
            .friction(friction)
            .restitution(restitution)
            .build();
        colliders.insert(collider);
    }

    // 壁セルに垂直 cuboid を足してボールの水平脱出を防ぐ。
    let (wall_friction, wall_restitution) = tile_material(TILE_WALL);
    for y in 0..MAP_H {
        for x in 0..MAP_W {
            if course.tile_at(x, y) != Some(TILE_WALL) {
                continue;
            }
            let collider = ColliderBuilder::cuboid(0.5, 0.5, WALL_CEILING_HALF as f32)
                .translation(Vector::new(
                    x as f32 + 0.5,
                    y as f32 + 0.5,
                    WALL_CEILING_HALF as f32,
                ))
                .friction(wall_friction)
                .restitution(wall_restitution)
                .build();
            colliders.insert(collider);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ball_falls_under_gravity() {
        let course = Course::generate(42);
        let mut phys = Physics::new(&course, [5.0, 20.0, 25.0]);
        for _ in 0..12 {
            phys.step(FIXED_DT);
        }
        let s = phys.ball_state();
        assert!(s.pos[2] < 25.0, "ball should fall, z={}", s.pos[2]);
        assert!(s.pos[2] > 19.0, "ball should not tunnel, z={}", s.pos[2]);
    }

    #[test]
    fn ball_rests_on_flat_tee() {
        let course = Course::generate(42);
        let mut phys = Physics::new(&course, [5.5, 20.0, 20.2]);
        for _ in 0..180 {
            phys.step(FIXED_DT);
        }
        let s = phys.ball_state();
        assert!(s.at_rest, "ball should come to rest on flat tee");
        assert!(
            (s.pos[2] - (20.0 + BALL_RADIUS)).abs() < 0.1,
            "ball rest z should be ~{}, got {}",
            20.0 + BALL_RADIUS,
            s.pos[2]
        );
    }

    #[test]
    fn launch_imparts_velocity() {
        let course = Course::generate(42);
        let mut phys = Physics::new(&course, [5.5, 20.0, 20.2]);
        phys.launch([5.0, 0.0, 0.0]);
        let s = phys.ball_state();
        assert!(
            (s.vel[0] - 5.0).abs() < 1e-3,
            "vx should be ~5.0, got {}",
            s.vel[0]
        );
    }

    #[test]
    fn triangulation_matches_heightmap_at_corners() {
        let course = Course::generate(42);
        let x = 60;
        let y = 20;
        let h = course.cell_heights(x, y);
        let tris = cell_triangles(&course, x, y);
        // tri[0] = NW, tri[1] = NE, tri[2] = SE
        assert!((tris[0][0].z as f64 - h.floor[0]).abs() < 1e-6);
        assert!((tris[0][1].z as f64 - h.floor[1]).abs() < 1e-6);
        assert!((tris[0][2].z as f64 - h.floor[3]).abs() < 1e-6);
        // tri2[0] = NW, tri2[1] = SE, tri2[2] = SW
        assert!((tris[1][0].z as f64 - h.floor[0]).abs() < 1e-6);
        assert!((tris[1][1].z as f64 - h.floor[3]).abs() < 1e-6);
        assert!((tris[1][2].z as f64 - h.floor[2]).abs() < 1e-6);
    }

    #[test]
    fn huge_dt_is_clamped_not_spiraling() {
        // `step(10.0)` は最大 15 ステップ相当にクランプされるはず。
        // panic しない & アキュムレータの残余が FIXED_DT 未満であることを確認。
        let course = Course::generate(42);
        let mut phys = Physics::new(&course, [5.0, 20.0, 25.0]);
        let t0 = std::time::Instant::now();
        phys.step(10.0);
        let elapsed = t0.elapsed();
        assert!(
            elapsed.as_secs_f64() < 1.0,
            "step(10.0) should clamp and return quickly, took {:?}",
            elapsed
        );
        assert!(phys.time_accumulator < FIXED_DT);
    }

    #[test]
    fn at_rest_becomes_true_after_stop() {
        let course = Course::generate(42);
        let mut phys = Physics::new(&course, [5.5, 20.0, 20.05]);
        for _ in 0..120 {
            phys.step(FIXED_DT);
        }
        assert!(phys.ball_state().at_rest);
    }
}
