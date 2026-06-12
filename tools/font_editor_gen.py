"""Generate the interactive font editor HTML (single file, vanilla JS).

PARITY-DISPATCHED OPTICAL MASTERS: the editor embeds BOTH nx75 design
files and dispatches per glyph scale k exactly like the codec
(crates/codec/src/px.rs raster_glyph):

- EVEN k -> the v1 master (design/glyph-font.v1.json) under the
  KERNEL-PULL law (per-anchor kernel swept along each active edge to
  the midpoint; isolated anchors at rest; cell mask)
- ODD k  -> the v2 master (design/glyph-font.v2.json) under the
  CONNECTION-KERNEL law (straight rects center-to-center, corner L1
  diamonds clipped to the anti-diagonal band, node quadrants, no
  mask)

EDITING CHOICE: edits affect the master matching the CURRENT UNDERLAY
PARITY — pick an even underlay k and you are editing v1, odd and you
are editing v2 (the toolbar shows which). The previews always render
each k through its own master, so both masters stay visible at all
times. Export emits one master at a time (paste into the matching
design/glyph-font.v{1,2}.json), reset restores the current master's
glyph from the file this generator embedded.
"""
import json
from pathlib import Path

font_v1 = json.loads(Path("design/glyph-font.v1.json").read_text())
font_v2 = json.loads(Path("design/glyph-font.v2.json").read_text())

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
#master{font-weight:bold}
</style></head><body>
<div id="glyphs"></div>
<div style="margin-top:6px">
mode: <button class="mode sel" id="m-px">pixels</button><button class="mode" id="m-conn">connections</button><button class="mode" id="m-kern">kernel sq/di</button>
&nbsp; underlay k: <select id="editk"><option>2</option><option>3</option><option selected>4</option><option>6</option><option>8</option><option>12</option></select>
&nbsp; editing <span id="master"></span>
&nbsp; <button id="export-v1">export v1 JSON</button> <button id="export-v2">export v2 JSON</button> <button id="reset">reset glyph</button>
</div>
<div class="hint">PARITY OPTICAL MASTERS: even k renders/edits v1 (kernel-pull), odd k renders/edits v2 (connection-kernel) &middot; pixels: click/drag toggles anchors &middot; connections: click a line (solid=on, faint dotted=off) &middot; kernel: click an anchor to flip &#9632;/&#9670;</div>
<div id="main">
  <div><canvas id="edit" width="480" height="672"></canvas></div>
  <div id="previews"></div>
</div>
<div id="strip"></div>
<textarea id="out" placeholder="exported JSON appears here"></textarea>
<script>
const FONTS = { v1: __FONTDATA_V1__, v2: __FONTDATA_V2__ };
const ALPHA = "23456789ABCDEFGHJKMNPQRSTUVWXYZ";
let state = { v1: {}, v2: {} };
for (const ver of ["v1", "v2"]) for (const ch of ALPHA) {
  const g = FONTS[ver][ch];
  state[ver][ch] = { px: g.px.map(row => [...row]), conn: {...g.conn}, kern: Object.fromEntries(Object.entries(g.kern).map(([k2, v]) => [k2, [...v]])) };
}
let cur = "K", mode = "px";

// The parity dispatch — the same law as crates/codec/src/px.rs
// raster_glyph: even k -> v1 kernel-pull, odd k -> v2
// connection-kernel.
function verFor(k){ return (k % 2 === 0) ? "v1" : "v2"; }
function editK(){ return parseInt(document.getElementById("editk").value); }
function curVer(){ return verFor(editK()); }
function rasterAny(ver, g, k){ return ver === "v1" ? rasterV1(g, k) : rasterV2(g, k); }

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
  // corners = [tl,tr,bl,br] — the resolution law BOTH masters share:
  // a kern override wins; else orth-touching/isolated -> full
  // square; else the bare quadrant-less node
  const k = g.kern[r+","+c];
  if (k) return k;
  let orth = false, any = false;
  for (const e of activeEdges(g)){
    if (!e.on) continue;
    const hit = (e.a[0]===r&&e.a[1]===c)||(e.b[0]===r&&e.b[1]===c);
    if (hit){ any = true; if (!e.diag) orth = true; }
  }
  return (orth || !any) ? [1,1,1,1] : [0,0,0,0];
}
function kernCovers(corners, dx, dy, half){
  if (Math.abs(dx)+Math.abs(dy) <= half) return true;
  if (Math.abs(dx) > half || Math.abs(dy) > half) return false;
  const ci = (dy < 0 ? 0 : 2) + (dx < 0 ? 0 : 1);
  return !!corners[ci];
}
function rasterV1(g, k){
  // KERNEL-PULL model (the TRUE v1 law): per-anchor kernel swept
  // along each active edge to the MIDPOINT; isolated anchors at
  // rest; cell mask = anchor cells + bridge cells of active
  // diagonal edges.
  const px = g.px, edges = activeEdges(g).filter(e=>e.on);
  const allowed = new Set();
  for (let r=0;r<7;r++) for (let c=0;c<5;c++) if (px[r][c]) allowed.add(r+","+c);
  for (const e of edges) if (e.diag){ allowed.add(e.a[0]+","+e.b[1]); allowed.add(e.b[0]+","+e.a[1]); }
  const sweeps = [], inked = new Set();
  for (const e of edges){
    for (const [me, other] of [[e.a,e.b],[e.b,e.a]]){
      const ax=(me[1]+0.5)*k, ay=(me[0]+0.5)*k;
      const mx=((me[1]+other[1])/2+0.5)*k, my=((me[0]+other[0])/2+0.5)*k;
      sweeps.push({ax, ay, vx:mx-ax, vy:my-ay, kern:kernel(g,me[0],me[1])});
      inked.add(me[0]+","+me[1]);
    }
  }
  for (let r=0;r<7;r++) for (let c=0;c<5;c++)
    if (px[r][c] && !inked.has(r+","+c))
      sweeps.push({ax:(c+0.5)*k, ay:(r+0.5)*k, vx:0, vy:0, kern:kernel(g,r,c)});
  const img = [];
  for (let j=0;j<7*k;j++) img.push(new Array(5*k).fill(0));
  const half = k/2;
  for (let j=0;j<7*k;j++) for (let i=0;i<5*k;i++){
    if (!allowed.has(Math.floor(j/k)+","+Math.floor(i/k))) continue;
    const x=i+0.5, y=j+0.5;
    for (const s of sweeps){
      const L2 = s.vx*s.vx + s.vy*s.vy;
      const t = L2===0 ? 0 : Math.max(0, Math.min(1, ((x-s.ax)*s.vx+(y-s.ay)*s.vy)/L2));
      if (kernCovers(s.kern, x-(s.ax+t*s.vx), y-(s.ay+t*s.vy), half)){ img[j][i]=1; break; }
    }
  }
  return img;
}
function rasterV2(g, k){
  // CONNECTION-KERNEL model: connections own center-to-center ink
  // with fixed shapes; nodes are pure edge-px config (quadrants).
  // straight = k-wide rect between node centers (inclusive) and
  // diagonal = L1 diamond radius k at the shared corner — it
  // inscribes the 2x2 cell block exactly, overflow is impossible.
  const px = g.px, edges = activeEdges(g).filter(e=>e.on);
  const img = [];
  for (let j=0;j<7*k;j++){ img.push(new Array(5*k).fill(0)); }
  const half = k/2;
  const stamps = [];
  for (const e of edges){
    if (!e.diag){
      const x1=(Math.min(e.a[1],e.b[1])+0.5)*k, x2=(Math.max(e.a[1],e.b[1])+0.5)*k;
      const y1=(Math.min(e.a[0],e.b[0])+0.5)*k, y2=(Math.max(e.a[0],e.b[0])+0.5)*k;
      stamps.push({t:"rect", x1, x2, y1, y2});
    } else {
      const cx = Math.max(e.a[1], e.b[1])*k;
      const cy = Math.max(e.a[0], e.b[0])*k;
      // anti-diagonal index sign: direction (1,1) -> dx-dy, else dx+dy
      const sameSign = (e.b[0]-e.a[0]) === (e.b[1]-e.a[1]);
      stamps.push({t:"diam", cx, cy, sameSign});
    }
  }
  for (let j=0;j<7*k;j++) for (let i=0;i<5*k;i++){
    const x=i+0.5, y=j+0.5;
    let on = false;
    for (const s of stamps){
      if (s.t === "rect"){
        if (s.y1 === s.y2){
          // horizontal: px centers between the node centers
          // inclusive, k-wide perpendicular
          if (x >= s.x1-1e-9 && x <= s.x2+1e-9 && Math.abs(y - s.y1) <= half){ on=true; break; }
        } else {
          if (y >= s.y1-1e-9 && y <= s.y2+1e-9 && Math.abs(x - s.x1) <= half){ on=true; break; }
        }
      } else {
        // corner diamond (radius k, reaches both node centers)
        // clipped to the k-row perpendicular band: chains render
        // constant-width, single corners become k-wide chamfers
        const dx = x-s.cx, dy = y-s.cy;
        const anti = s.sameSign ? Math.abs(dx-dy) : Math.abs(dx+dy);
        if (Math.abs(dx)+Math.abs(dy) <= k + 1e-9 && anti <= k-1 + 1e-9){ on=true; break; }
      }
    }
    if (!on){
      const cr = Math.floor(j/k), cc = Math.floor(i/k);
      if (cr<7 && cc<5 && px[cr][cc]){
        const kc = kernel(g, cr, cc);
        const dx = x-(cc+0.5)*k, dy = y-(cr+0.5)*k;
        const ci = (dy<0?0:2)+(dx<0?0:1);
        if (kc[ci]) on = true;
      }
    }
    if (img[j] && on) img[j][i]=1;
  }
  return img;
}

const edit = document.getElementById("edit"), ectx = edit.getContext("2d");
function drawEdit(){
  // The edit canvas shows the master the underlay parity selects —
  // its anchors, edges and kernels are what the mouse edits.
  const ver = curVer(), g = state[ver][cur];
  ectx.clearRect(0,0,480,672);
  const sc = 480/5;
  const k = editK(), R = rasterAny(ver, g, k);
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
  document.getElementById("master").textContent = ver + " (" + (ver === "v1" ? "even k, kernel-pull" : "odd k, connection-kernel") + ")";
}
function drawPreviews(){
  // Each preview renders its k through ITS OWN master — the parity
  // dispatch — so v1 and v2 stay visible side by side.
  const div = document.getElementById("previews");
  div.innerHTML = "";
  for (const k of [2,3,4,6]){
    const ver = verFor(k), g = state[ver][cur];
    const cv = document.createElement("canvas");
    const z = {2:5,3:4,4:3,6:2}[k];
    cv.width = 5*k*z; cv.height = 7*k*z;
    const ctx = cv.getContext("2d");
    const R = rasterAny(ver, g, k);
    ctx.fillStyle="#000";
    for (let j=0;j<7*k;j++) for (let i=0;i<5*k;i++) if (R[j][i]) ctx.fillRect(i*z,j*z,z,z);
    cv.style.cursor = "pointer";
    cv.title = "k="+k+" -> "+ver+" (click to set underlay / edit this master)";
    cv.onclick = ()=>{ document.getElementById("editk").value = String(k); sync(); };
    div.appendChild(cv);
  }
}
function drawStrip(){
  // The strip renders at k=3 — odd, so the v2 master via the same
  // dispatch.
  const div = document.getElementById("strip");
  div.innerHTML = "";
  for (const ch of ALPHA){
    const k=3, ver=verFor(k), z=2, cv=document.createElement("canvas");
    cv.width=5*k*z; cv.height=7*k*z; cv.title=ch;
    const ctx=cv.getContext("2d");
    const R = rasterAny(ver, state[ver][ch], k);
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
  const g=state[curVer()][cur];
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
  const g=state[curVer()][cur];
  if (r>=0&&r<7&&c>=0&&c<5 && g.px[r][c]!==dragVal){ g.px[r][c]=dragVal; sync(); }
};
window.onmouseup = ()=>{ dragging=false; };
document.getElementById("m-px").onclick = e=>{mode="px"; selMode(e.target);};
document.getElementById("m-conn").onclick = e=>{mode="conn"; selMode(e.target);};
document.getElementById("m-kern").onclick = e=>{mode="kern"; selMode(e.target);};
function selMode(t){ document.querySelectorAll(".mode").forEach(b=>b.classList.remove("sel")); t.classList.add("sel"); }
document.getElementById("editk").onchange = sync;
document.getElementById("export-v1").onclick = ()=>{
  document.getElementById("out").value = JSON.stringify(state.v1);
};
document.getElementById("export-v2").onclick = ()=>{
  document.getElementById("out").value = JSON.stringify(state.v2);
};
document.getElementById("reset").onclick = ()=>{
  const ver = curVer();
  const g = FONTS[ver][cur];
  state[ver][cur] = { px: g.px.map(row=>[...row]), conn: {...g.conn}, kern: Object.fromEntries(Object.entries(g.kern).map(([k2,v]) => [k2,[...v]])) };
  sync();
};
sync();
</script></body></html>
"""
html = html.replace("__FONTDATA_V1__", json.dumps(font_v1))
html = html.replace("__FONTDATA_V2__", json.dumps(font_v2))
out = Path("labels/typography-bench/font-editor.html")
out.write_text(html)
print(f"-> {out}")
