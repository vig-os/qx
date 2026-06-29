// Dual-engine scan bench — one camera, both decoders, live A/B.
//
// Lifts the v1 scanner's live-camera feel: the stream shows continuously and
// detected codes are marked up in real time (fast zxing every animation
// frame). The slow rxing `decode_image` runs on a throttled background
// sampler so it measures the A/B without ever stuttering the live view.

import { decodeBoth, initEngines, liveDetectZxing, type Point2D } from "./dual-decode";
import {
  emptyTallies,
  hitRate,
  percentile,
  record,
  rxingParityOnZxingHits,
  type Tallies,
} from "./stats";

const SNAPSHOT_MAX_PX = 1280;
const AB_SAMPLE_MS = 500; // how often the slow rxing A/B samples a frame

const $ = (id: string) => document.getElementById(id)!;
const video = $("cam") as HTMLVideoElement;
const overlay = $("overlay") as HTMLCanvasElement;
const abCanvas = document.createElement("canvas"); // off-screen A/B frame buffer

const tallies: Tallies = emptyTallies();
let frame = 0;
let running = false;
let lastFrameJpeg: Blob | null = null;
let autoCaptureDiverge = false;

function fmt(n: number, d = 1): string {
  return n.toFixed(d);
}

// ---- live overlay (every animation frame, zxing only) ----

function drawOverlay(corners: Point2D[] | null, label?: string): void {
  const ctx = overlay.getContext("2d")!;
  if (overlay.width !== video.videoWidth || overlay.height !== video.videoHeight) {
    overlay.width = video.videoWidth;
    overlay.height = video.videoHeight;
  }
  ctx.clearRect(0, 0, overlay.width, overlay.height);
  if (!corners || corners.length < 3) return;
  ctx.strokeStyle = "#19e08a";
  ctx.lineWidth = Math.max(3, overlay.width / 200);
  ctx.beginPath();
  corners.forEach((p, i) => (i ? ctx.lineTo(p.x, p.y) : ctx.moveTo(p.x, p.y)));
  ctx.closePath();
  ctx.stroke();
  for (const p of corners) {
    ctx.beginPath();
    ctx.arc(p.x, p.y, ctx.lineWidth * 1.5, 0, Math.PI * 2);
    ctx.fillStyle = "#19e08a";
    ctx.fill();
  }
  if (label) {
    const cx = corners.reduce((s, p) => s + p.x, 0) / corners.length;
    const top = Math.min(...corners.map((p) => p.y));
    ctx.font = `bold ${Math.max(20, overlay.width / 30)}px ui-monospace, monospace`;
    ctx.textAlign = "center";
    ctx.lineWidth = 4;
    ctx.strokeStyle = "#000";
    ctx.strokeText(label, cx, top - 12);
    ctx.fillStyle = "#19e08a";
    ctx.fillText(label, cx, top - 12);
  }
}

async function liveTick(): Promise<void> {
  if (!running) return;
  if (video.videoWidth) {
    // zxing reads the video element directly — fast, every frame.
    const hit = await liveDetectZxing(video);
    drawOverlay(hit?.corners ?? null, hit?.value);
  }
  requestAnimationFrame(() => void liveTick());
}

// ---- throttled rxing A/B sampler (background, never blocks the live view) ----

async function abSample(): Promise<void> {
  if (!running || !video.videoWidth) return;
  const vw = video.videoWidth;
  const vh = video.videoHeight;
  const longEdge = Math.max(vw, vh);
  const scale = longEdge > SNAPSHOT_MAX_PX ? SNAPSHOT_MAX_PX / longEdge : 1;
  abCanvas.width = Math.round(vw * scale);
  abCanvas.height = Math.round(vh * scale);
  const actx = abCanvas.getContext("2d", { willReadFrequently: true })!;
  actx.drawImage(video, 0, 0, abCanvas.width, abCanvas.height);
  // Hand both engines the IDENTICAL pixels (same ImageData → same luma).
  const img = actx.getImageData(0, 0, abCanvas.width, abCanvas.height);

  const r = await decodeBoth(img);
  frame++;
  record(tallies, frame, r);
  renderStats();

  if (autoCaptureDiverge && r.diverge) {
    abCanvas.toBlob((b) => b && downloadFrame(b, `diverge-${frame}`), "image/jpeg", 0.95);
  }
  abCanvas.toBlob((b) => (lastFrameJpeg = b), "image/jpeg", 0.95);
}

function renderStats(): void {
  const z = tallies.zxing;
  const r = tallies.rxing;
  const parity = rxingParityOnZxingHits(tallies);
  $("stats").innerHTML = `
    <table>
      <tr><th>A/B samples</th><th>zxing</th><th>rxing</th></tr>
      <tr><td>frames</td><td>${z.frames}</td><td>${r.frames}</td></tr>
      <tr><td>hits</td><td>${z.hits}</td><td>${r.hits}</td></tr>
      <tr><td>hit-rate</td><td>${fmt(hitRate(z))}%</td><td>${fmt(hitRate(r))}%</td></tr>
      <tr><td>p50 ms</td><td>${fmt(percentile(z.latencies, 50))}</td><td>${fmt(percentile(r.latencies, 50))}</td></tr>
      <tr><td>p95 ms</td><td>${fmt(percentile(z.latencies, 95))}</td><td>${fmt(percentile(r.latencies, 95))}</td></tr>
    </table>
    <div class="gate">
      <strong>rxing parity on zxing's hits:</strong>
      ${parity.agree}/${parity.zxingHitFrames} = <b>${fmt(parity.parityPct)}%</b>
      <div class="muted">Must reach ~100% before zxing can be dropped.</div>
    </div>
    <div>agree: ${tallies.agree} &nbsp; diverge: ${tallies.diverge}</div>`;

  if (tallies.divergences.length) {
    $("log").innerHTML =
      "<strong>Divergences (one engine read, the other missed):</strong><br>" +
      tallies.divergences
        .slice(-12)
        .reverse()
        .map(
          (d) =>
            `#${d.frame} &nbsp; zxing=<code>${d.zxing ?? "—"}</code> &nbsp; rxing=<code>${d.rxing ?? "—"}</code>`,
        )
        .join("<br>");
  }
}

function downloadFrame(blob: Blob, name: string): void {
  const a = document.createElement("a");
  a.href = URL.createObjectURL(blob);
  a.download = `${name}.jpg`;
  a.click();
  setTimeout(() => URL.revokeObjectURL(a.href), 1000);
}

async function start(): Promise<void> {
  $("status").textContent = "loading decoders…";
  const info = await initEngines();
  $("status").textContent = `zxing ${info.zxingVersion} + rxing (Rust) — requesting camera…`;
  const stream = await navigator.mediaDevices.getUserMedia({
    video: { facingMode: "environment", width: { ideal: 1920 } },
  });
  video.srcObject = stream;
  await new Promise<void>((res) => (video.onloadedmetadata = () => res()));
  await video.play();
  $("status").textContent = `live — ${video.videoWidth}×${video.videoHeight}`;
  $("startBtn").textContent = "● live";
  (($("startBtn") as HTMLButtonElement).disabled = true);
  running = true;
  void liveTick();
  // Background A/B sampler — independent cadence so the slow rxing decode
  // never stutters the live overlay.
  const pump = async () => {
    if (!running) return;
    await abSample();
    setTimeout(() => void pump(), AB_SAMPLE_MS);
  };
  void pump();
}

$("startBtn").addEventListener("click", () =>
  void start().catch((e) => {
    $("status").textContent = "error: " + (e as Error).message;
  }),
);
$("captureBtn").addEventListener("click", () => {
  if (lastFrameJpeg) downloadFrame(lastFrameJpeg, `corpus-frame-${frame}`);
});
$("autoBtn").addEventListener("click", (e) => {
  autoCaptureDiverge = !autoCaptureDiverge;
  (e.target as HTMLButtonElement).textContent = autoCaptureDiverge
    ? "auto-capture divergences: ON"
    : "auto-capture divergences: OFF";
});
