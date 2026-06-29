// Dual-engine scan bench — one camera, both decoders, live A/B.
//
// Lifts the production scanner's camera loop (web/src/ui/scanner.ts) into a
// measurement harness: every frame is painted once and fanned to zxing-wasm
// AND the Rust rxing `decode_image`, recording hit-rate, latency, and — the
// number that gates dropping zxing — rxing's parity on the frames zxing
// reads. Capture interesting frames straight into the CI corpus.

import { decodeBoth, initEngines } from "./dual-decode";
import {
  emptyTallies,
  hitRate,
  percentile,
  record,
  rxingParityOnZxingHits,
  type Tallies,
} from "./stats";

const SNAPSHOT_MAX_PX = 1280;

const $ = (id: string) => document.getElementById(id)!;
const video = $("cam") as HTMLVideoElement;
const overlay = $("overlay") as HTMLCanvasElement;
const work = document.createElement("canvas"); // off-screen frame buffer

const tallies: Tallies = emptyTallies();
let frame = 0;
let running = false;
let lastFrameJpeg: Blob | null = null;
let autoCaptureDiverge = false;

function fmt(n: number, d = 1): string {
  return n.toFixed(d);
}

function renderStats(): void {
  const z = tallies.zxing;
  const r = tallies.rxing;
  const parity = rxingParityOnZxingHits(tallies);
  $("stats").innerHTML = `
    <table>
      <tr><th></th><th>zxing</th><th>rxing</th></tr>
      <tr><td>frames</td><td>${z.frames}</td><td>${r.frames}</td></tr>
      <tr><td>hits</td><td>${z.hits}</td><td>${r.hits}</td></tr>
      <tr><td>hit-rate</td><td>${fmt(hitRate(z))}%</td><td>${fmt(hitRate(r))}%</td></tr>
      <tr><td>p50 ms</td><td>${fmt(percentile(z.latencies, 50))}</td><td>${fmt(percentile(r.latencies, 50))}</td></tr>
      <tr><td>p95 ms</td><td>${fmt(percentile(z.latencies, 95))}</td><td>${fmt(percentile(r.latencies, 95))}</td></tr>
    </table>
    <div class="gate">
      <strong>rxing parity on zxing's hits:</strong>
      ${parity.agree}/${parity.zxingHitFrames} = <b>${fmt(parity.parityPct)}%</b>
      <div class="muted">This must reach ~100% before zxing can be dropped.</div>
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

function drawOverlay(corners?: { x: number; y: number }[], label?: string): void {
  const ctx = overlay.getContext("2d")!;
  overlay.width = video.videoWidth;
  overlay.height = video.videoHeight;
  ctx.clearRect(0, 0, overlay.width, overlay.height);
  if (!corners || corners.length < 3) return;
  ctx.strokeStyle = "#19e08a";
  ctx.lineWidth = Math.max(3, overlay.width / 240);
  ctx.beginPath();
  corners.forEach((p, i) => (i ? ctx.lineTo(p.x, p.y) : ctx.moveTo(p.x, p.y)));
  ctx.closePath();
  ctx.stroke();
  if (label) {
    const cx = corners.reduce((s, p) => s + p.x, 0) / corners.length;
    const top = Math.min(...corners.map((p) => p.y));
    ctx.font = `${Math.max(18, overlay.width / 36)}px ui-monospace, monospace`;
    ctx.fillStyle = "#19e08a";
    ctx.textAlign = "center";
    ctx.fillText(label, cx, top - 8);
  }
}

async function loop(): Promise<void> {
  if (!running) return;
  const vw = video.videoWidth;
  const vh = video.videoHeight;
  if (vw && vh) {
    const longEdge = Math.max(vw, vh);
    const scale = longEdge > SNAPSHOT_MAX_PX ? SNAPSHOT_MAX_PX / longEdge : 1;
    work.width = Math.round(vw * scale);
    work.height = Math.round(vh * scale);
    work.getContext("2d")!.drawImage(video, 0, 0, work.width, work.height);

    const r = await decodeBoth(work);
    frame++;
    record(tallies, frame, r);
    // scale zxing corners (work-space) back to video-space for the overlay
    const corners = r.zxing.corners?.map((p) => ({ x: p.x / scale, y: p.y / scale }));
    drawOverlay(corners, r.zxing.value ?? r.rxing.value ?? undefined);
    renderStats();

    if (autoCaptureDiverge && r.diverge) {
      work.toBlob((b) => b && downloadFrame(b, `diverge-${frame}`), "image/jpeg", 0.95);
    }
    work.toBlob((b) => (lastFrameJpeg = b), "image/jpeg", 0.95);
  }
  requestAnimationFrame(() => void loop());
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
  running = true;
  void loop();
}

$("startBtn").addEventListener("click", () => void start().catch((e) => {
  $("status").textContent = "error: " + (e as Error).message;
}));
$("captureBtn").addEventListener("click", () => {
  if (lastFrameJpeg) downloadFrame(lastFrameJpeg, `corpus-frame-${frame}`);
});
$("autoBtn").addEventListener("click", (e) => {
  autoCaptureDiverge = !autoCaptureDiverge;
  (e.target as HTMLButtonElement).textContent = autoCaptureDiverge
    ? "auto-capture divergences: ON"
    : "auto-capture divergences: OFF";
});
