"""Generate the interactive font editor HTML (single file, vanilla JS)."""
import json
from pathlib import Path

font = json.loads(Path("design/glyph-font.v1.json").read_text())
F = {ch: ["".join(str(v) for v in row) for row in g["px"]] for ch, g in font.items()}

html = """<!DOCTYPE html>
<html><head><meta charset="utf-8"><title>part-registry font editor</title>
<style>
body{font-family:monospace;margin:14px;background:#fafafa}
#glyphs button{margin:1px;width:26px;height:26px;font-weight:bold;cursor:pointer}
#glyphs button.sel{background:#222;color:#fff}
.mode{margin:2px;padding:4px 10px;cursor:pointer}
.mode.sel{background:#222;color:#fff}
#main{display:flex;gap:18px;margin-top:10px;align-items:flex-start}
canvas{background:#fff;border:1px solid #ccc;image-rendering:pixelated}
#previews canvas{display:block;margin-bottom:8px}
#strip{margin-top:14px;white-space:nowrap;overflow-x:auto}
#strip canvas{margin-right:4px}
textarea{width:100%;height:90px;font-size:10px}
.hint{color:#666;font-size:12px}
</style></head><body>
<div id="glyphs"></div>
<div style="margin-top:6px">
mode: <button class="mode sel" id="m-px">pixels</button><button class="mode" id="m-conn">connections</button><button class="mode" id="m-kern">kernel sq/di</button>
&nbsp; <label><input type="checkbox" id="mask" checked> cell clip mask</label>
&nbsp; underlay k: <select id="editk"><option>2</option><option>3</option><option selected>4</option><option>6</option><option>8</option><option>12</option></select>
&nbsp; <button id="export">export JSON</button> <button id="reset">reset glyph</button>
</div>
<div class="hint">pixels: click/drag toggles anchors &middot; connections: click a line (solid=on, faint dotted=off) &middot; kernel: click an anchor to flip &#9632;/&#9670;</div>
<div id="main">
  <div><canvas id="edit" width="480" height="672"></canvas></div>
  <div id="previews"></div>
</div>
<div id="strip"></div>
<textarea id="out" placeholder="exported JSON appears here"></textarea>
<script>
const FONT = __FONTDATA__;
const F = Object.fromEntries(Object.entries(FONT).map(([ch, g]) => [ch, g.px.map(r => r.join(""))]));
const ALPHA = "23456789ABCDEFGHJKMNPQRSTUVWXYZ";
let state = {};
for (const ch of ALPHA) {
  const g = FONT[ch];
  state[ch] = { px: g.px.map(row => [...row]), conn: {...g.conn}, kern: Object.fromEntries(Object.entries(g.kern).map(([k2, v]) => [k2, [...v]])) };
}
let cur = "K", mode = "px";

function at(px, r, c){ return (r>=0&&r<7&&c>=0&&c<5) ? px[r][c] : 0; }
function edgeKey(a, b){ return a[0]+","+a[1]+"-"+b[0]+","+b[1]; }
function candEdges(px){
  const es = [];
  for (let r=0;r<7;r++) for (let c=0;c<5;c++){
    if (!px[r][c]) continue;
    for (const [dr,dc] of [[0,1],[1,0],[1,1],[1,-1]]){
      if (!at(px,r+dr,c+dc)) continue;
      const diag = dr!==0 && dc!==0;
      let def = true;
      if (diag){ def = !(at(px,r,c+dc) || at(px,r+dr,c)); }
      es.push({a:[r,c], b:[r+dr,c+dc], diag, def});
    }
  }
  return es;
}
function activeEdges(g){
  return candEdges(g.px).map(e => {
    const k = edgeKey(e.a, e.b);
    return {...e, on: (k in g.conn) ? g.conn[k] : e.def};
  });
}
function kernel(g, r, c){
  // corners = [tl,tr,bl,br]; the kernel IS the anchor cell's ink:
  // orth-touching/isolated -> full square; pure-diagonal -> diamond
  // plus the corners ALONG its active runs (ribbon continuity)
  const k = g.kern[r+","+c];
  if (k) return k;
  let orth = false, any = false;
  const corners = [0,0,0,0];
  for (const e of activeEdges(g)){
    if (!e.on) continue;
    let other = null;
    if (e.a[0]===r&&e.a[1]===c) other = e.b;
    else if (e.b[0]===r&&e.b[1]===c) other = e.a;
    if (!other) continue;
    any = true;
    if (!e.diag){ orth = true; }
    else {
      const dr = other[0]-r, dc = other[1]-c;
      corners[(dr<0?0:2)+(dc<0?0:1)] = 1;
    }
  }
  if (orth || !any) return [1,1,1,1];
  return [0,0,0,0];
}
function kernCovers(corners, dx, dy, half){
  if (Math.abs(dx)+Math.abs(dy) <= half) return true;
  if (Math.abs(dx) > half || Math.abs(dy) > half) return false;
  const ci = (dy < 0 ? 0 : 2) + (dx < 0 ? 0 : 1);
  return !!corners[ci];
}
function raster(g, k, useMask){
  const px = g.px, edges = activeEdges(g).filter(e=>e.on);
  const allowed = new Set();
  for (let r=0;r<7;r++) for (let c=0;c<5;c++) if (px[r][c]) allowed.add(r+","+c);
  for (const e of edges) if (e.diag){
    allowed.add(e.a[0]+","+e.b[1]);
    allowed.add(e.b[0]+","+e.a[1]);
  }
  const img = [];
  for (let j=0;j<7*k;j++){ img.push(new Array(5*k).fill(0)); }
  const half = k/2;
  // THE model: every anchor's kernel is pulled along each of its
  // active vectors to the edge MIDPOINT (the far half belongs to the
  // far anchor's kernel; midpoints sit on cell boundaries, so kernels
  // own their cells). Tips cap with their kernel at rest; junctions
  // are hole-free by construction (their kernel centers every sweep).
  const sweeps = [];
  const inked = new Set();
  // constant-derivative law: pass-through DIAGONAL anchors (exactly
  // two collinear diagonal edges) get NO rest-stamp — their cells are
  // band-owned; stamps poke 0.35px past the band and jitter the edge
  const adeg = {};
  for (const e of edges){
    for (const q of [e.a, e.b]){
      const key = q[0]+","+q[1];
      (adeg[key] = adeg[key] || []).push(e);
    }
  }
  const bandOwned = new Set();
  const diagTip = new Set();
  for (const key in adeg){
    const es2 = adeg[key];
    if (es2.length === 2 && es2[0].diag && es2[1].diag){
      const d0 = [es2[0].b[0]-es2[0].a[0], es2[0].b[1]-es2[0].a[1]];
      const d1 = [es2[1].b[0]-es2[1].a[0], es2[1].b[1]-es2[1].a[1]];
      if (Math.abs(d0[0]*d1[1] - d0[1]*d1[0]) < 1e-9) bandOwned.add(key);
    }
    if (es2.length === 1 && es2[0].diag) diagTip.add(key);
  }
  for (const e of edges){
    // canonical edge frame: the normal and the ink balance are
    // computed ONCE from the stored a->b orientation, and BOTH
    // half-sweeps reuse the same nx/ny/outSign — a centered band
    // can never flip its outer side at the edge midpoint
    let outSign = 1, oneSided = false, cnx = 0, cny = 0;
    // run vs corner-connector: a diagonal edge with NO collinear
    // diagonal continuation at either end is a corner treatment and
    // keeps the slim corridor-exact sweep the glyphs were authored
    // under; only true runs take the k-row band
    let isRun = false;
    if (e.diag){
      const dr0 = e.b[0]-e.a[0], dc0 = e.b[1]-e.a[1];
      const beyondB = (e.b[0]+dr0)+","+(e.b[1]+dc0);
      const beforeA = (e.a[0]-dr0)+","+(e.a[1]-dc0);
      for (const e2 of edges){
        if (!e2.diag || e2 === e) continue;
        for (const q of [e2.a, e2.b]){
          const qk = q[0]+","+q[1];
          if (qk === beyondB || qk === beforeA){ isRun = true; break; }
        }
        if (isRun) break;
      }
    }
    if (e.diag){
      const ax0=(e.a[1]+0.5)*k, ay0=(e.a[0]+0.5)*k;
      const bx0=(e.b[1]+0.5)*k, by0=(e.b[0]+0.5)*k;
      const len0 = Math.hypot(bx0-ax0, by0-ay0);
      cnx = -(by0-ay0)/len0; cny = (bx0-ax0)/len0;
      let bal = 0;
      for (let rr=0;rr<7;rr++) for (let cc2=0;cc2<5;cc2++){
        if (!px[rr][cc2]) continue;
        const d = ((cc2+0.5)*k-ax0)*cnx + ((rr+0.5)*k-ay0)*cny;
        if (Math.abs(d) > 1e-9) bal += Math.sign(d);
      }
      // outside = the side with less ink mass = negative dsig; at
      // k=3 the diagonal gains its extra row there, corridor-exact
      outSign = bal > 0 ? 1 : -1;
      // LOCAL one-sided trigger: only when a bridge cell of this
      // edge is orthogonally flush to FOREIGN ink (phantom risk),
      // and the band shifts AWAY from that ink. Bowl corners with
      // no neighbors stay centered (stamps stay under the band).
      let foreignSide = 0;
      for (const [br2, bc2] of [[e.a[0], e.b[1]], [e.b[0], e.a[1]]]){
        let foreign = false;
        for (const [dr2, dc2] of [[-1,0],[1,0],[0,-1],[0,1]]){
          const nr2 = br2+dr2, nc2 = bc2+dc2;
          if (nr2<0||nr2>=7||nc2<0||nc2>=5 || !px[nr2][nc2]) continue;
          const isEnd2 = (nr2===e.a[0]&&nc2===e.a[1])||(nr2===e.b[0]&&nc2===e.b[1]);
          if (!isEnd2){ foreign = true; break; }
        }
        if (foreign){
          const db = ((bc2+0.5)*k-ax0)*cnx + ((br2+0.5)*k-ay0)*cny;
          foreignSide += Math.sign(db);
        }
      }
      if (foreignSide !== 0){
        outSign = foreignSide > 0 ? 1 : -1;
        oneSided = true;
      }
    }
    for (const [me, other] of [[e.a, e.b], [e.b, e.a]]){
      const ax=(me[1]+0.5)*k, ay=(me[0]+0.5)*k;
      const mx=((me[1]+other[1])/2+0.5)*k, my=((me[0]+other[0])/2+0.5)*k;
      sweeps.push({ax, ay, vx: mx-ax, vy: my-ay, diag: e.diag, isRun, nx: cnx, ny: cny, outSign, oneSided, bandOwned: bandOwned.has(me[0]+","+me[1]), diagTip: diagTip.has(me[0]+","+me[1]), kern: kernel(g, me[0], me[1])});
      inked.add(me[0]+","+me[1]);
    }
  }
  for (let r=0;r<7;r++) for (let c=0;c<5;c++){
    if (px[r][c] && !inked.has(r+","+c)){
      sweeps.push({ax:(c+0.5)*k, ay:(r+0.5)*k, vx:0, vy:0, kern: kernel(g,r,c)});
    }
  }
  for (let j=0;j<7*k;j++) for (let i=0;i<5*k;i++){
    const cr = Math.floor(j/k), cc = Math.floor(i/k);
    if (useMask && !allowed.has(cr+","+cc)) continue;
    const x=i+0.5, y=j+0.5;
    let on = false;
    for (const s of sweeps){
      // pulled BODY is round (L2 <= k/2): uniform width-k envelope at
      // every angle (diamond pulled = thin, square pulled = greedy) —
      // the KERNEL shape applies at rest: cell ownership, endplates,
      // corner chips
      const L2 = s.vx*s.vx + s.vy*s.vy;
      const t = L2 === 0 ? 0 : Math.max(0, Math.min(1, ((x-s.ax)*s.vx + (y-s.ay)*s.vy)/L2));
      if (t > 0 && L2 > 0){
        if (s.diag){
          // pixel-font diagonal law: exactly k px per anti-diagonal
          // row — x/y extent k (stem-equal), band windows measured
          // against the CANONICAL a->b normal stored on the sweep
          const dsig = ((x-s.ax)*s.nx + (y-s.ay)*s.ny) * s.outSign;
          // k anti-diagonal rows total; parity remainder and the k=3
          // bonus row always land on the OUTSIDE
          if (!s.isRun){
            // corner connector: slim diamond-pull (L1 to the
            // half-segment), corridor-exact — the authored look
            const t2 = Math.max(0, Math.min(1, (((x-s.ax)*s.vx)+((y-s.ay)*s.vy))/L2));
            const qx2 = s.ax + t2*s.vx, qy2 = s.ay + t2*s.vy;
            if (Math.abs(x-qx2) + Math.abs(y-qy2) <= k/2 + 1e-6){ on=true; break; }
          } else if (k <= 2){
            // n=2 floor: perpendicular thickness = kernel size
            // (full-width pulled body; the mask absorbs the spill)
            if (Math.abs(dsig) <= k/2 + 1e-6){ on=true; break; }
          } else if (s.oneSided){
            // ink nearby: the band hugs the OUTSIDE of the anchor
            // line — inner boundary IS the line, so it can never
            // overshoot toward a neighboring stroke (no guard needed)
            const rows = k + (k === 3 ? 1 : 0);
            const odd = (k % 2) === 1;
            const lo = -((rows - (odd ? 1 : 0.5))/Math.SQRT2 + 1e-6);
            if (dsig >= lo && dsig <= 1e-6){ on=true; break; }
          } else {
            const odd = (k % 2) === 1;
            const innerRows = odd ? (k-1)/2 : (k/2 - 1);
            const outerRows = (odd ? (k-1)/2 : k/2) + (k === 3 ? 1 : 0);
            const hw = innerRows/Math.SQRT2 + 1e-6;
            const lo = -(outerRows/Math.SQRT2 + 1e-6);
            if (dsig >= lo && dsig <= hw){ on=true; break; }
          }
        } else {
          const dx = x-(s.ax+t*s.vx), dy = y-(s.ay+t*s.vy);
          if (Math.hypot(dx, dy) <= half){ on=true; break; }
        }
      }
      if (!s.bandOwned){
        const dx2 = x-s.ax, dy2 = y-s.ay;
        if (s.diagTip){
          // pure diagonal tip: corners-only endplate (no diamond —
          // the band end is the cap; the chip is the outward block)
          if (Math.abs(dx2)<=half && Math.abs(dy2)<=half){
            const ci = (dy2<0?0:2)+(dx2<0?0:1);
            if (s.kern[ci]){ on=true; break; }
          }
        } else if (kernCovers(s.kern, dx2, dy2, half)){ on=true; break; }
      }
    }
    if (on) img[j][i]=1;
  }
  return img;
}

const Z = 96, edit = document.getElementById("edit"), ectx = edit.getContext("2d");
function drawEdit(){
  const g = state[cur];
  ectx.clearRect(0,0,480,672);
  const cell = Z*5/5;
  const sc = 480/5;
  const k = parseInt(document.getElementById("editk").value), R = raster(g, k, document.getElementById("mask").checked);
  const pz = 480/(5*k);
  ectx.fillStyle = "#eee";
  for (let j=0;j<7*k;j++) for (let i=0;i<5*k;i++)
    if (R[j][i]){ ectx.fillRect(i*pz, j*pz, pz+0.5, pz+0.5); }
  ectx.strokeStyle = "#bbb";
  for (let r=0;r<=7;r++){ ectx.beginPath(); ectx.moveTo(0,r*sc); ectx.lineTo(480,r*sc); ectx.stroke(); }
  for (let c=0;c<=5;c++){ ectx.beginPath(); ectx.moveTo(c*sc,0); ectx.lineTo(c*sc,672); ectx.stroke(); }
  const cen = (r,c)=>[(c+0.5)*sc,(r+0.5)*sc];
  for (const e of activeEdges(g)){
    const [x1,y1]=cen(...e.a), [x2,y2]=cen(...e.b);
    ectx.lineWidth = e.on?5:2;
    ectx.setLineDash(e.on?[]:[4,5]);
    ectx.strokeStyle = e.on ? (e.diag?"#c22":"#181") : "#caa";
    ectx.beginPath(); ectx.moveTo(x1,y1); ectx.lineTo(x2,y2); ectx.stroke();
  }
  ectx.setLineDash([]);
  for (let r=0;r<7;r++) for (let c=0;c<5;c++){
    if (!g.px[r][c]) continue;
    const [x,y]=cen(r,c);
    const kc = kernel(g,r,c), S = 12;
    ectx.fillStyle="#000";
    ectx.beginPath(); ectx.moveTo(x,y-S); ectx.lineTo(x+S,y); ectx.lineTo(x,y+S); ectx.lineTo(x-S,y); ectx.closePath(); ectx.fill();
    const quads = [[-S,-S],[0,-S],[-S,0],[0,0]];
    for (let ci=0; ci<4; ci++){
      if (!kc[ci]) continue;
      ectx.fillRect(x+quads[ci][0], y+quads[ci][1], S, S);
    }
    ectx.strokeStyle="#fff"; ectx.strokeRect(x-2,y-2,4,4);
  }
}
function drawPreviews(){
  const g = state[cur], div = document.getElementById("previews");
  div.innerHTML = "";
  for (const k of [2,3,4,6]){
    const cv = document.createElement("canvas");
    const z = {2:5,3:4,4:3,6:2}[k];
    cv.width = 5*k*z; cv.height = 7*k*z;
    const ctx = cv.getContext("2d");
    const R = raster(g, k, document.getElementById("mask").checked);
    ctx.fillStyle="#000";
    for (let j=0;j<7*k;j++) for (let i=0;i<5*k;i++) if (R[j][i]) ctx.fillRect(i*z,j*z,z,z);
    cv.style.cursor = "pointer";
    cv.title = "k="+k+" (click to set underlay)";
    cv.onclick = ()=>{ document.getElementById("editk").value = String(k); sync(); };
    div.appendChild(cv);
  }
}
function drawStrip(){
  const div = document.getElementById("strip");
  div.innerHTML = "";
  for (const ch of ALPHA){
    const k=3, z=2, cv=document.createElement("canvas");
    cv.width=5*k*z; cv.height=7*k*z; cv.title=ch;
    const ctx=cv.getContext("2d");
    const R = raster(state[ch], k, true);
    ctx.fillStyle="#000";
    for (let j=0;j<7*k;j++) for (let i=0;i<5*k;i++) if (R[j][i]) ctx.fillRect(i*z,j*z,z,z);
    cv.onclick=()=>{cur=ch; sync();};
    div.appendChild(cv);
  }
}
function sync(){
  document.querySelectorAll("#glyphs button").forEach(b=>b.classList.toggle("sel", b.textContent===cur));
  drawEdit(); drawPreviews(); drawStrip();
}
const gl = document.getElementById("glyphs");
for (const ch of ALPHA){
  const b=document.createElement("button"); b.textContent=ch;
  b.onclick=()=>{cur=ch; sync();};
  gl.appendChild(b);
}
let dragging=false, dragVal=null;
edit.onmousedown = e => {
  const rect=edit.getBoundingClientRect();
  const x=e.clientX-rect.left, y=e.clientY-rect.top;
  const sc=480/5, c=Math.floor(x/sc), r=Math.floor(y/sc);
  const g=state[cur];
  if (mode==="px"){
    dragging=true; dragVal = g.px[r]&&g.px[r][c]?0:1;
    if(r<7&&c<5){ g.px[r][c]=dragVal; sync(); }
  } else if (mode==="kern"){
    if (r<7&&c<5&&g.px[r][c]){
      const key=r+","+c;
      const cx=(c+0.5)*sc, cy=(r+0.5)*sc;
      const cur_ = [...kernel(g,r,c)];
      const dx=x-cx, dy=y-cy;
      if (Math.hypot(dx,dy) < 12){
        const allOn = cur_.every(v=>v);
        g.kern[key] = allOn ? [0,0,0,0] : [1,1,1,1];
      } else {
        const ci = (dy<0?0:2)+(dx<0?0:1);
        cur_[ci] = cur_[ci]?0:1;
        g.kern[key] = cur_;
      }
      sync();
    }
  } else {
    let best=null, bd=1e9;
    for (const ed of activeEdges(g)){
      const [x1,y1]=[(ed.a[1]+0.5)*sc,(ed.a[0]+0.5)*sc], [x2,y2]=[(ed.b[1]+0.5)*sc,(ed.b[0]+0.5)*sc];
      const mx=(x1+x2)/2, my=(y1+y2)/2, d=Math.hypot(x-mx,y-my);
      if (d<bd){ bd=d; best=ed; }
    }
    if (best && bd<40){ const k=edgeKey(best.a,best.b); g.conn[k]=!best.on; sync(); }
  }
};
edit.onmousemove = e => {
  if (!dragging || mode!=="px") return;
  const rect=edit.getBoundingClientRect();
  const sc=480/5, c=Math.floor((e.clientX-rect.left)/sc), r=Math.floor((e.clientY-rect.top)/sc);
  if (r>=0&&r<7&&c>=0&&c<5 && state[cur].px[r][c]!==dragVal){ state[cur].px[r][c]=dragVal; sync(); }
};
window.onmouseup = ()=>{ dragging=false; };
document.getElementById("m-px").onclick = e=>{mode="px"; selMode(e.target);};
document.getElementById("m-conn").onclick = e=>{mode="conn"; selMode(e.target);};
document.getElementById("m-kern").onclick = e=>{mode="kern"; selMode(e.target);};
function selMode(t){ document.querySelectorAll(".mode").forEach(b=>b.classList.remove("sel")); t.classList.add("sel"); }
document.getElementById("mask").onchange = sync;
document.getElementById("editk").onchange = sync;
document.getElementById("export").onclick = ()=>{
  document.getElementById("out").value = JSON.stringify(state);
};
document.getElementById("reset").onclick = ()=>{
  state[cur] = { px: F[cur].map(row=>[...row].map(Number)), conn:{}, kern:{} };
  sync();
};
sync();
</script></body></html>
"""
html = html.replace("__FONTDATA__", json.dumps(font))
out = Path("labels/typography-bench/font-editor.html")
out.write_text(html)
print(f"-> {out}")
