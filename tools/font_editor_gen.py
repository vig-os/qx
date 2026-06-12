"""Generate the interactive font editor HTML (single file, vanilla JS)."""
import json
from pathlib import Path

font = json.loads(Path("design/glyph-font.v2.json").read_text())
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
  // CONNECTION-KERNEL model: connections own center-to-center ink
  // with fixed shapes; nodes are pure edge-px config (quadrants).
  // straight = k-wide rect between node centers (inclusive);
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
