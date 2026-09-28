// A small perspective camera. World units are design pixels with the
// screen's axes: x right, y DOWN, z away from the viewer. A camera placed
// REF units in front of the z=0 plane shows that plane exactly 1:1, so
// layouts can be authored in screen coordinates and then moved in depth.

export const FOV = 40 * Math.PI / 180;
export const F = 540 / Math.tan(FOV / 2); // focal length in design px
export const REF = F; // distance at which 1 world unit = 1 screen px

const sub = (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const cross = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const norm = a => {
  const l = Math.hypot(a[0], a[1], a[2]) || 1;
  return [a[0] / l, a[1] / l, a[2] / l];
};

// eye/target in world space; roll in radians; fov optional.
export function camera(eye, target, roll = 0, fov = FOV) {
  const fwd = norm(sub(target, eye));
  let down = [0, 1, 0];
  down = norm(sub(down, fwd.map(v => v * dot(down, fwd))));
  let right = cross(down, fwd);
  if (roll) {
    const c = Math.cos(roll), s = Math.sin(roll);
    const r2 = right.map((v, i) => v * c + down[i] * s);
    const d2 = down.map((v, i) => v * c - right[i] * s);
    right = r2;
    down = d2;
  }
  const f = 540 / Math.tan(fov / 2);
  return { eye, target, fwd, right, down, f, near: 20 };
}

// The canonical front camera: shows z=0 at 1:1.
export const front = (cx = 960, cy = 540, dist = REF) => camera([cx, cy, -dist], [cx, cy, 0]);

// Orbit around a target: yaw about the vertical axis, pitch up/down.
export function orbit(target, dist, yaw, pitch = 0, roll = 0, fov = FOV) {
  const cp = Math.cos(pitch);
  const eye = [
    target[0] + Math.sin(yaw) * cp * dist,
    target[1] - Math.sin(pitch) * dist,
    target[2] - Math.cos(yaw) * cp * dist,
  ];
  return camera(eye, target, roll, fov);
}

// Project a world point. Returns {x, y, s, z, ok}: screen position, scale
// (screen px per world unit at that depth), view depth, visibility.
export function project(cam, p) {
  const d = sub(p, cam.eye);
  const z = dot(d, cam.fwd);
  const x = dot(d, cam.right), y = dot(d, cam.down);
  if (z < cam.near) return { x: 0, y: 0, s: 0, z, ok: false };
  const s = cam.f / z;
  return { x: 960 + x * s, y: 540 + y * s, s, z, ok: true };
}
