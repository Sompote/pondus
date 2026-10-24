/* Pondus website scripts: navigation, lightbox, hero animation, analysis lab. */
(function () {
  'use strict';

  /* ---------- small helpers ---------- */
  const $ = (s, r) => (r || document).querySelector(s);
  const $$ = (s, r) => Array.from((r || document).querySelectorAll(s));
  const fmt = (v, d) => {
    if (!isFinite(v)) return '–';
    const a = Math.abs(v);
    const digits = d != null ? d : a >= 1000 ? 0 : a >= 100 ? 1 : a >= 10 ? 2 : 3;
    return v.toLocaleString(undefined, { minimumFractionDigits: digits, maximumFractionDigits: digits });
  };
  const css = (name) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();

  /* ---------- nav ---------- */
  const toggle = $('.nav-toggle');
  if (toggle) toggle.addEventListener('click', () => $('.nav-links').classList.toggle('open'));
  $$('.nav-links a').forEach(a => a.addEventListener('click', () => $('.nav-links').classList.remove('open')));

  /* ---------- reveal on scroll ---------- */
  if ('IntersectionObserver' in window) {
    const io = new IntersectionObserver(es => es.forEach(e => { if (e.isIntersecting) { e.target.classList.add('in'); io.unobserve(e.target); } }), { threshold: 0.08 });
    $$('.reveal').forEach(el => io.observe(el));
  } else $$('.reveal').forEach(el => el.classList.add('in'));

  /* ---------- lightbox ---------- */
  const lb = $('#lightbox');
  if (lb) {
    const img = $('img', lb), cap = $('figcaption', lb);
    const open = (src, text) => { img.src = src; cap.textContent = text || ''; lb.classList.add('open'); document.body.style.overflow = 'hidden'; };
    const close = () => { lb.classList.remove('open'); document.body.style.overflow = ''; img.src = ''; };
    $$('[data-zoom]').forEach(el => el.addEventListener('click', () => {
      const i = el.tagName === 'IMG' ? el : $('img', el);
      open(el.dataset.zoom || i.src, el.dataset.caption || i.alt);
    }));
    lb.addEventListener('click', e => { if (e.target === lb || e.target.tagName === 'BUTTON') close(); });
    document.addEventListener('keydown', e => { if (e.key === 'Escape') close(); });
  }

  /* ---------- OS tabs and copy buttons (manual) ---------- */
  $$('.os-tabs').forEach(group => {
    const name = group.dataset.group;
    $$('button', group).forEach(b => b.addEventListener('click', () => {
      $$('button', group).forEach(x => x.classList.toggle('active', x === b));
      $$(`.os-panel[data-group="${name}"]`).forEach(p => p.classList.toggle('active', p.dataset.os === b.dataset.os));
    }));
  });
  $$('pre').forEach(pre => {
    const wrap = document.createElement('div'); wrap.className = 'copy-wrap';
    pre.parentNode.insertBefore(wrap, pre); wrap.appendChild(pre);
    const b = document.createElement('button'); b.className = 'copy'; b.textContent = 'Copy';
    b.addEventListener('click', () => { navigator.clipboard && navigator.clipboard.writeText(pre.innerText).then(() => { b.textContent = 'Copied'; setTimeout(() => b.textContent = 'Copy', 1400); }); });
    wrap.appendChild(b);
  });

  /* ---------- manual TOC highlight ---------- */
  const toc = $('.toc');
  if (toc && 'IntersectionObserver' in window) {
    const links = $$('a', toc);
    const heads = links.map(a => $(a.getAttribute('href'))).filter(Boolean);
    const io2 = new IntersectionObserver(es => {
      es.forEach(e => { if (e.isIntersecting) { links.forEach(a => a.classList.toggle('current', a.getAttribute('href') === '#' + e.target.id)); } });
    }, { rootMargin: '-20% 0px -70% 0px' });
    heads.forEach(h => io2.observe(h));
  }

  /* =====================================================================
     HERO ANIMATION: the agent types a request, calls tools, and a frame is
     built, analysed (moment diagrams) and designed (ratio colours).
     ===================================================================== */
  const heroCanvas = $('#hero-canvas');
  if (heroCanvas && !matchMedia('(prefers-reduced-motion: reduce)').matches) {
    const ctx = heroCanvas.getContext('2d');
    const prompt = 'Build a 3-storey concrete frame, 3 bays of 6 m by 2 bays of 5 m, 150 mm slabs, office live load, and design the beams and slabs';
    const tools = [
      { t: 3.2, name: 'add_frame', note: '36 columns, 51 beams, 18 slabs' },
      { t: 6.0, name: 'add_loads', note: 'dead 4.2 kPa, live 2.5 kPa' },
      { t: 7.4, name: 'analyze', note: '534 elements, 24 ms, reactions balance' },
      { t: 10.6, name: 'design', note: 'ACI 318-19 beams and slabs' },
      { t: 13.6, name: 'write_report', note: 'calculation report, 7 pages' },
    ];
    const DUR = 17.5;
    // geometry: 3 bays x 6 m, 2 bays x 5 m, 3 storeys x 3.5 m
    const xs = [0, 6, 12, 18], ys = [0, 5, 10], zs = [0, 3.5, 7, 10.5];
    const cols = [], beams = [], slabs = [];
    xs.forEach((x, i) => ys.forEach((y, j) => zs.slice(0, -1).forEach((z, k) => cols.push({ a: [x, y, z], b: [x, y, zs[k + 1]], order: k * 12 + j * 4 + i }))));
    zs.slice(1).forEach((z, k) => {
      for (let j = 0; j < ys.length; j++) for (let i = 0; i < xs.length - 1; i++) beams.push({ a: [xs[i], ys[j], z], b: [xs[i + 1], ys[j], z], order: k * 20 + i + j * 3, L: 6 });
      for (let i = 0; i < xs.length; i++) for (let j = 0; j < ys.length - 1; j++) beams.push({ a: [xs[i], ys[j], z], b: [xs[i], ys[j + 1], z], order: k * 20 + 9 + i * 2 + j, L: 5 });
      for (let i = 0; i < xs.length - 1; i++) for (let j = 0; j < ys.length - 1; j++) slabs.push({ p: [[xs[i], ys[j], z], [xs[i + 1], ys[j], z], [xs[i + 1], ys[j + 1], z], [xs[i], ys[j + 1], z]], order: k * 6 + i * 2 + j });
    });
    // deterministic pseudo ratios for the design colouring
    const ratioOf = (m) => { const s = Math.sin(m.a[0] * 12.9898 + m.a[1] * 78.233 + m.a[2] * 37.719) * 43758.5453; return 0.45 + (s - Math.floor(s)) * 0.5; };
    const ratioColor = r => r < 0.7 ? '#4fc36a' : r < 0.9 ? '#e6c34a' : r < 1.0 ? '#ef8a3c' : '#ef5350';

    let W = 0, H = 0, dpr = 1;
    function resize() {
      dpr = Math.min(2, window.devicePixelRatio || 1);
      const r = heroCanvas.getBoundingClientRect();
      W = Math.max(1, Math.round(r.width)); H = Math.max(1, Math.round(r.height));
      heroCanvas.width = W * dpr; heroCanvas.height = H * dpr; ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    }
    resize(); addEventListener('resize', resize);

    function project(p, ang) {
      // simple isometric-like projection with a slow orbit
      const cx = Math.cos(ang), sx = Math.sin(ang);
      const x = p[0] - 9, y = p[1] - 5, z = p[2];
      const X = x * cx - y * sx, Y = x * sx + y * cx;
      const s = Math.min(W, H * 1.3) / 36;
      const ox = W * 0.56, oy = H * 0.62;
      return [ox + X * s, oy + Y * s * 0.45 - z * s * 0.85, Y];
    }
    const ease = t => t < 0 ? 0 : t > 1 ? 1 : t * t * (3 - 2 * t);
    const start = performance.now();

    function frame(now) {
      const t = ((now - start) / 1000) % DUR;
      const ang = -0.55 + Math.sin(t / DUR * Math.PI * 2) * 0.12;
      ctx.clearRect(0, 0, W, H);
      // canvas dots like the app
      ctx.fillStyle = '#3a3a3e';
      for (let gx = 20; gx < W * 0.74; gx += 22) for (let gy = 40; gy < H; gy += 22) ctx.fillRect(gx, gy, 1.2, 1.2);

      // phases
      const buildT = (t - 3.2) / 2.6;      // 3.2–5.8 members appear
      const loadT = (t - 6.0) / 1.2;       // loads
      const anaT = (t - 7.4) / 1.4;        // moment diagrams grow
      const desT = (t - 10.6) / 1.6;       // colours
      const repT = (t - 13.6) / 1.2;       // report badge

      // slabs
      slabs.forEach(sl => {
        const a = ease(buildT * 4 - sl.order / 18 * 3 - 0.9);
        if (a <= 0) return;
        ctx.beginPath(); sl.p.forEach((p, i) => { const q = project(p, ang); i ? ctx.lineTo(q[0], q[1]) : ctx.moveTo(q[0], q[1]); }); ctx.closePath();
        let fill = 'rgba(120,130,150,0.18)';
        if (desT > 0) { const r = ratioOf({ a: sl.p[0] }); const c = ratioColor(r * 0.9); fill = hexA(c, 0.22 * ease(desT)); }
        ctx.fillStyle = fill; ctx.globalAlpha = a; ctx.fill(); ctx.globalAlpha = 1;
      });
      // columns
      cols.forEach(c => {
        const a = ease(buildT * 4 - c.order / 36 * 3);
        if (a <= 0) return;
        const p = project(c.a, ang), q = project(c.b, ang);
        const col = desT > 0 ? mix('#c9cdd6', ratioColor(ratioOf(c) * 0.8), ease(desT)) : '#c9cdd6';
        line(p, [p[0] + (q[0] - p[0]) * a, p[1] + (q[1] - p[1]) * a], col, 2.2);
        if (c.a[2] === 0 && a > 0.9) { ctx.fillStyle = '#4fc36a'; ctx.beginPath(); ctx.moveTo(p[0], p[1] - 1); ctx.lineTo(p[0] - 5, p[1] + 5); ctx.lineTo(p[0] + 5, p[1] + 5); ctx.closePath(); ctx.fill(); }
      });
      // beams with moment diagrams
      beams.forEach(b => {
        const a = ease(buildT * 4 - b.order / 60 * 3 - 0.4);
        if (a <= 0) return;
        const p = project(b.a, ang), q = project(b.b, ang);
        const r = ratioOf(b);
        const col = desT > 0 ? mix('#dfe3ea', ratioColor(r), ease(desT)) : '#dfe3ea';
        line(p, [p[0] + (q[0] - p[0]) * a, p[1] + (q[1] - p[1]) * a], col, 2.4);
        // loads: small arrows along the beam
        if (loadT > 0 && anaT < 1.4) {
          const la = ease(loadT) * (1 - ease(anaT - 0.6));
          ctx.strokeStyle = `rgba(90,163,245,${0.7 * la})`; ctx.lineWidth = 1;
          for (let s = 0.15; s < 1; s += 0.17) {
            const x = p[0] + (q[0] - p[0]) * s, y = p[1] + (q[1] - p[1]) * s;
            ctx.beginPath(); ctx.moveTo(x, y - 12 * la); ctx.lineTo(x, y - 2); ctx.stroke();
          }
        }
        // moment diagram: parabola drawn on the tension side (sagging below the beam)
        if (anaT > 0) {
          const g = ease(anaT);
          const h = (b.L === 6 ? 16 : 11) * g;
          const n = 16;
          ctx.beginPath(); ctx.moveTo(p[0], p[1]);
          for (let i = 0; i <= n; i++) {
            const s = i / n; const x = p[0] + (q[0] - p[0]) * s, y = p[1] + (q[1] - p[1]) * s;
            // hogging at the ends (negative, drawn above), sagging in the middle (drawn below)
            const m = -(1 / 12) + s * (1 - s) / 2; // wL²-normalised fixed-end shape
            ctx.lineTo(x, y + m * 9 * h);
          }
          ctx.lineTo(q[0], q[1]); ctx.closePath();
          ctx.fillStyle = 'rgba(240,140,70,0.28)'; ctx.fill();
          ctx.strokeStyle = 'rgba(240,140,70,0.9)'; ctx.lineWidth = 1; ctx.stroke();
        }
      });

      // ---- chat panel on the right ----
      const px = W * 0.70, pw = W * 0.30 - 10;
      ctx.fillStyle = '#1f1f21'; ctx.fillRect(px, 28, pw + 10, H - 28);
      ctx.fillStyle = '#2a2a2c'; ctx.fillRect(px, 28, pw + 10, 26);
      ctx.fillStyle = '#a0cdff'; ctx.font = `600 ${Math.max(10, W * 0.012)}px Inter, system-ui, sans-serif`; ctx.fillText('✦ Agent', px + 12, 46);
      ctx.fillStyle = '#8b8f99'; ctx.fillText('Properties', px + 12 + ctx.measureText('✦ Agent   ').width, 46);
      // typed prompt bubble
      const typeT = Math.min(1, t / 2.6);
      const shown = prompt.slice(0, Math.floor(prompt.length * typeT));
      const fontPx = Math.max(10, W * 0.0128);
      ctx.font = `${fontPx}px Inter, system-ui, sans-serif`;
      const lines = wrapText(ctx, shown + (typeT < 1 ? '▍' : ''), pw - 36);
      const lh = fontPx * 1.4; const bh = lines.length * lh + 16;
      roundRect(px + 14, 66, pw - 18, bh, 8, '#24405f');
      ctx.fillStyle = '#e4e8f0'; lines.forEach((l, i) => ctx.fillText(l, px + 24, 66 + 12 + fontPx + i * lh - 2));
      // tool chips
      let y = 66 + bh + 14;
      tools.forEach(tool => {
        const a = ease((t - tool.t) / 0.5);
        if (a <= 0) return;
        ctx.globalAlpha = a;
        roundRect(px + 14, y, pw - 18, lh * 2 + 10, 8, '#2a2a2e');
        ctx.fillStyle = '#5aa3f5'; ctx.font = `600 ${fontPx}px "JetBrains Mono", ui-monospace, monospace`; ctx.fillText('⚙ ' + tool.name, px + 24, y + fontPx + 6);
        ctx.fillStyle = '#9ea4b1'; ctx.font = `${fontPx * 0.88}px Inter, system-ui, sans-serif`; ctx.fillText(truncate(ctx, tool.note, pw - 52), px + 24, y + fontPx * 2 + 9);
        const done = (t - tool.t) > 1.1;
        ctx.fillStyle = done ? '#4fc36a' : '#e6c34a'; ctx.beginPath(); ctx.arc(px + pw - 14, y + lh + 5, 4, 0, 7); ctx.fill();
        ctx.globalAlpha = 1;
        y += lh * 2 + 18;
      });
      // final reply
      if (repT > 0.4) {
        const a = ease((repT - 0.4) / 0.6);
        ctx.globalAlpha = a; ctx.font = `${fontPx}px Inter, system-ui, sans-serif`;
        const rl = wrapText(ctx, 'Done. 51 beams and 18 slabs pass ACI 318-19 (worst ratio 0.97, B 16). Reactions balance the loads. Report written next to the model.', pw - 36);
        roundRect(px + 14, y, pw - 18, rl.length * lh + 16, 8, '#2a2a2c');
        ctx.fillStyle = '#dedee0'; rl.forEach((l, i) => ctx.fillText(l, px + 24, y + 12 + fontPx + i * lh - 2));
        ctx.globalAlpha = 1;
      }
      // status line
      ctx.fillStyle = '#1d1d1f'; ctx.fillRect(0, H - 22, W * 0.70, 22);
      ctx.fillStyle = '#9a9ea8'; ctx.font = `${Math.max(9, W * 0.011)}px Inter, system-ui, sans-serif`;
      const status = t < 3.2 ? 'Ready' : t < 7.4 ? '48 nodes · 87 members · 18 slabs · 12 supports' : t < 10.6 ? 'Analysed 534 elements in 24 ms; reactions balance the applied loads' : 'Design: ACI 318-19 · 51 beams, 18 slabs · worst ratio 0.97';
      ctx.fillText(status, 12, H - 7);
      // legend for ratio colours
      if (desT > 0) {
        ctx.globalAlpha = ease(desT);
        const lx = 14, ly = 40; ctx.font = `${Math.max(9, W * 0.011)}px Inter, system-ui, sans-serif`;
        [['< 0.7', '#4fc36a'], ['< 0.9', '#e6c34a'], ['< 1.0', '#ef8a3c'], ['fails', '#ef5350']].forEach((e, i) => { ctx.fillStyle = e[1]; ctx.fillRect(lx, ly + i * 16, 10, 10); ctx.fillStyle = '#b9bdc7'; ctx.fillText(e[0], lx + 16, ly + i * 16 + 9); });
        ctx.globalAlpha = 1;
      }
      requestAnimationFrame(frame);
    }
    requestAnimationFrame(frame);

    function line(p, q, col, w) { ctx.strokeStyle = col; ctx.lineWidth = w; ctx.lineCap = 'round'; ctx.beginPath(); ctx.moveTo(p[0], p[1]); ctx.lineTo(q[0], q[1]); ctx.stroke(); }
    function roundRect(x, y, w, h, r, fill) { ctx.fillStyle = fill; ctx.beginPath(); ctx.roundRect(x, y, w, h, r); ctx.fill(); }
    function wrapText(c, text, maxW) { const words = text.split(' '); const out = []; let cur = ''; words.forEach(w => { const test = cur ? cur + ' ' + w : w; if (c.measureText(test).width > maxW && cur) { out.push(cur); cur = w; } else cur = test; }); if (cur) out.push(cur); return out; }
    function truncate(c, text, maxW) { if (c.measureText(text).width <= maxW) return text; let t = text; while (t.length > 1 && c.measureText(t + '…').width > maxW) t = t.slice(0, -1); return t + '…'; }
    function hexA(hex, a) { const n = parseInt(hex.slice(1), 16); return `rgba(${n >> 16},${(n >> 8) & 255},${n & 255},${a})`; }
    function mix(h1, h2, k) { const a = parseInt(h1.slice(1), 16), b = parseInt(h2.slice(1), 16); const ch = s => Math.round(((a >> s) & 255) * (1 - k) + ((b >> s) & 255) * k); return `rgb(${ch(16)},${ch(8)},${ch(0)})`; }
  } else if (heroCanvas) {
    // reduced motion: draw a static poster
    heroCanvas.parentElement.style.background = `#262628 url(assets/img/features/moments.png) center/cover no-repeat`;
  }

  /* =====================================================================
     ANALYSIS LAB
     ===================================================================== */
  const lab = $('#lab');
  if (!lab) return;
  $$('.tabs button', lab).forEach(b => b.addEventListener('click', () => {
    $$('.tabs button', lab).forEach(x => x.classList.toggle('active', x === b));
    $$('.panel', lab).forEach(p => p.classList.toggle('active', p.id === b.dataset.panel));
    redrawAll();
  }));
  function redrawAll() { drawBeam(); drawSpectrum(); drawModes(); }

  /* ---- shared plotting helpers (SVG) ---- */
  function svgPlot(el, w, h) { el.setAttribute('viewBox', `0 0 ${w} ${h}`); el.innerHTML = ''; return el; }
  function node(tag, attrs, text) { const n = document.createElementNS('http://www.w3.org/2000/svg', tag); for (const k in attrs) n.setAttribute(k, attrs[k]); if (text != null) n.textContent = text; return n; }
  const colors = () => ({ text: css('--text-dim') || '#555', faint: css('--line') || '#ddd', ink: css('--ink') || '#16213a', blue: css('--blue') || '#378ef0', teal: css('--teal') || '#0fa9b4', orange: '#e8742c', ok: css('--ok') || '#3aa65a', err: css('--error') || '#d9463f' });

  /* ------------------------------------------------------------------
     1. BEAM: simply supported, fixed-fixed or cantilever under a UDL and a
        point load. Closed-form superposition; sagging positive; moments are
        drawn on the tension side.
     ------------------------------------------------------------------ */
  const beamIds = ['b-type', 'b-L', 'b-w', 'b-P', 'b-a', 'b-b', 'b-h', 'b-E'];
  beamIds.forEach(id => { const el = $('#' + id); if (el) el.addEventListener('input', drawBeam); });
  function beamFns(type, L, w, P, a) {
    const b = L - a;
    if (type === 'ss') {
      return {
        M: x => w * x * (L - x) / 2 + (x < a ? P * b * x / L : P * a * (L - x) / L),
        V: x => w * L / 2 - w * x + (x < a ? P * b / L : -P * a / L),
        dEI: x => w * x * (L ** 3 - 2 * L * x * x + x ** 3) / 24 + (x < a ? P * b * x * (L * L - b * b - x * x) / (6 * L) : P * a * (L - x) * (2 * L * x - x * x - a * a) / (6 * L)),
        R: [w * L / 2 + P * b / L, w * L / 2 + P * a / L], name: 'Simply supported'
      };
    }
    if (type === 'ff') {
      const MA = -P * a * b * b / (L * L), RA = P * b * b * (3 * a + b) / L ** 3, RB = P * a * a * (a + 3 * b) / L ** 3;
      return {
        M: x => w * (6 * L * x - 6 * x * x - L * L) / 12 + MA + RA * x - (x > a ? P * (x - a) : 0),
        V: x => w * L / 2 - w * x + (x < a ? RA : RA - P),
        dEI: x => w * x * x * (L - x) ** 2 / 24 + (x <= a ? P * b * b * x * x * (3 * a * L - 3 * a * x - b * x) / (6 * L ** 3) : P * a * a * (L - x) ** 2 * (3 * b * L - 3 * b * (L - x) - a * (L - x)) / (6 * L ** 3)),
        R: [w * L / 2 + RA, w * L / 2 + RB], name: 'Fixed at both ends'
      };
    }
    // cantilever fixed at x = 0
    return {
      M: x => -w * (L - x) ** 2 / 2 - (x < a ? P * (a - x) : 0),
      V: x => w * (L - x) + (x < a ? P : 0),
      dEI: x => w * x * x * (6 * L * L - 4 * L * x + x * x) / 24 + (x <= a ? P * x * x * (3 * a - x) / 6 : P * a * a * (3 * x - a) / 6),
      R: [w * L + P, 0], name: 'Cantilever'
    };
  }
  function drawBeam() {
    const svg = $('#beam-plot'); if (!svg || !$('#panel-beam').classList.contains('active')) return;
    const type = $('#b-type').value, L = +$('#b-L').value, w = +$('#b-w').value, P = +$('#b-P').value;
    let a = Math.min(L, Math.max(0, +$('#b-a').value));
    const bmm = +$('#b-b').value, hmm = +$('#b-h').value, E = +$('#b-E').value; // mm, mm, MPa
    const I = bmm * hmm ** 3 / 12 / 1e12; // m^4
    const EI = E * 1e3 * I; // kN·m²  (MPa = kN/m² × 1e3)
    const f = beamFns(type, L, w, P, a);
    const N = 240, xsv = [], Ms = [], Vs = [], ds = [];
    for (let i = 0; i <= N; i++) { const x = L * i / N; xsv.push(x); Ms.push(f.M(x)); Vs.push(f.V(x)); ds.push(f.dEI(x) / EI * 1000); }
    const Mmax = Math.max(...Ms), Mmin = Math.min(...Ms), Vmax = Math.max(...Vs.map(Math.abs)), dmax = Math.max(...ds.map(Math.abs));
    const c = colors();
    const W = 760, H = 420, pad = 54, gw = W - 2 * pad;
    svgPlot(svg, W, H);
    const rows = [
      { y0: 30, h: 110, data: Ms, label: 'Bending moment M (kN·m), drawn on the tension side', col: c.orange, flip: true },
      { y0: 170, h: 100, data: Vs, label: 'Shear V (kN)', col: c.blue, flip: false },
      { y0: 300, h: 90, data: ds, label: 'Deflection δ (mm)', col: c.teal, flip: true },
    ];
    rows.forEach(r => {
      const mx = Math.max(1e-9, ...r.data.map(Math.abs));
      const base = r.y0 + r.h / 2;
      const sy = v => base + (r.flip ? v : -v) / mx * (r.h / 2 - 16);
      svg.appendChild(node('text', { x: pad, y: r.y0 - 8, fill: c.text, 'font-size': 12, 'font-weight': 600 }, r.label));
      // member line
      svg.appendChild(node('line', { x1: pad, y1: base, x2: pad + gw, y2: base, stroke: c.ink, 'stroke-width': 2.5 }));
      let d = `M${pad},${base}`;
      r.data.forEach((v, i) => d += ` L${pad + gw * i / N},${sy(v)}`);
      d += ` L${pad + gw},${base} Z`;
      svg.appendChild(node('path', { d, fill: r.col, 'fill-opacity': .25, stroke: r.col, 'stroke-width': 1.6 }));
      // extremes
      const iMax = r.data.indexOf(Math.max(...r.data)), iMin = r.data.indexOf(Math.min(...r.data));
      [[iMax, Math.max(...r.data)], [iMin, Math.min(...r.data)]].forEach(([i, v], k) => {
        if (Math.abs(v) < mx * 0.02) return;
        const x = pad + gw * i / N, y = sy(v);
        svg.appendChild(node('circle', { cx: x, cy: y, r: 3, fill: r.col }));
        svg.appendChild(node('text', { x: Math.min(W - pad, Math.max(pad, x)) + (i > N / 2 ? -6 : 6), y: y + (r.flip === (v > 0) ? 14 : -6), fill: c.text, 'font-size': 11, 'text-anchor': i > N / 2 ? 'end' : 'start' }, fmt(v, 1)));
      });
    });
    // supports and loads sketch on the moment row
    const base0 = rows[0].y0 + rows[0].h / 2;
    const sup = (x, kind) => {
      if (kind === 'pin') svg.appendChild(node('path', { d: `M${x},${base0} l-8,14 h16 z`, fill: c.ink }));
      else if (kind === 'roller') { svg.appendChild(node('path', { d: `M${x},${base0} l-8,12 h16 z`, fill: c.ink })); svg.appendChild(node('circle', { cx: x, cy: base0 + 16, r: 3, fill: c.ink })); }
      else svg.appendChild(node('rect', { x: x - 4, y: base0 - 22, width: 5, height: 44, fill: c.ink }));
    };
    if (type === 'ss') { sup(pad, 'pin'); sup(pad + gw, 'roller'); } else if (type === 'ff') { sup(pad, 'fixed'); sup(pad + gw, 'fixed'); } else sup(pad, 'fixed');
    if (P > 0) { const x = pad + gw * a / L; svg.appendChild(node('path', { d: `M${x},${base0 - 34} v22 m-5,-7 l5,7 l5,-7`, stroke: c.err, 'stroke-width': 2, fill: 'none' })); svg.appendChild(node('text', { x: x + 6, y: base0 - 26, fill: c.err, 'font-size': 11 }, `P = ${fmt(P, 0)} kN`)); }
    if (w > 0) { svg.appendChild(node('text', { x: pad + gw - 4, y: base0 - 26, fill: c.blue, 'font-size': 11, 'text-anchor': 'end' }, `w = ${fmt(w, 1)} kN/m`)); }
    svg.appendChild(node('text', { x: pad + gw / 2, y: H - 10, fill: c.text, 'font-size': 12, 'text-anchor': 'middle' }, `${f.name} · L = ${fmt(L, 2)} m · ${bmm} × ${hmm} mm · E = ${fmt(E, 0)} MPa`));
    // results
    const ratio = dmax > 0 ? L * 1000 / dmax : Infinity;
    const res = $('#beam-results');
    res.innerHTML = '';
    [
      ['M max (sagging)', fmt(Mmax, 1) + ' kN·m'],
      ['M min (hogging)', fmt(Mmin, 1) + ' kN·m'],
      ['V max', fmt(Vmax, 1) + ' kN'],
      ['Reactions', type === 'cant' ? `${fmt(f.R[0], 1)} kN` : `${fmt(f.R[0], 1)} / ${fmt(f.R[1], 1)} kN`],
      ['δ max', fmt(dmax, 2) + ' mm'],
      ['Span / δ', isFinite(ratio) ? 'L / ' + fmt(ratio, 0) : '–', ratio >= 360 ? 'ok' : 'bad'],
    ].forEach(r => { const d = document.createElement('div'); if (r[2]) d.className = r[2]; d.innerHTML = `<b>${r[1]}</b><span>${r[0]}</span>`; res.appendChild(d); });
    $('#beam-check').textContent = isFinite(ratio) ? (ratio >= 360 ? `Passes the L/360 deflection limit that Pondus applies to live load (gross section, no cracking or creep here).` : `Exceeds L/360 (the live-load limit Pondus checks). Pondus would deepen the beam in 50 mm steps with resize.`) : '';
  }

  /* ------------------------------------------------------------------
     2. RESPONSE SPECTRUM (ASCE 7-22 §11.4.5 and §12.8, the shape Pondus
        uses for its seismic analysis). Two-parameter design spectrum,
        approximate period Ta = Ct·hn^x with the Cu·Ta cap, Cs and V.
     ------------------------------------------------------------------ */
  ['s-sds', 's-sd1', 's-tl', 's-r', 's-ie', 's-hn', 's-ct', 's-T', 's-W'].forEach(id => { const el = $('#' + id); if (el) el.addEventListener('input', drawSpectrum); });
  function Sa(T, SDS, SD1, TL) {
    const T0 = 0.2 * SD1 / SDS, TS = SD1 / SDS;
    if (T < T0) return SDS * (0.4 + 0.6 * T / T0);
    if (T <= TS) return SDS;
    if (T <= TL) return SD1 / T;
    return SD1 * TL / (T * T);
  }
  function Cu(SD1) { // ASCE 7-22 Table 12.8-1, linear between the rows
    const pts = [[0.1, 1.7], [0.15, 1.6], [0.2, 1.5], [0.3, 1.4], [0.4, 1.4]];
    if (SD1 <= 0.1) return 1.7; if (SD1 >= 0.4) return 1.4;
    for (let i = 0; i < pts.length - 1; i++) if (SD1 <= pts[i + 1][0]) { const [a, b] = pts[i], [c, d] = pts[i + 1]; return b + (d - b) * (SD1 - a) / (c - a); }
    return 1.4;
  }
  function drawSpectrum() {
    const svg = $('#spec-plot'); if (!svg || !$('#panel-spec').classList.contains('active')) return;
    const SDS = +$('#s-sds').value, SD1 = +$('#s-sd1').value, TL = +$('#s-tl').value, R = +$('#s-r').value, Ie = +$('#s-ie').value;
    const hn = +$('#s-hn').value, ctx = $('#s-ct').value.split(','), Ct = +ctx[0], xexp = +ctx[1], Tmodel = +$('#s-T').value, Wt = +$('#s-W').value;
    const Ta = Ct * Math.pow(hn, xexp), cu = Cu(SD1), Tcap = cu * Ta;
    const Tuse = Tmodel > 0 ? Math.min(Tmodel, Tcap) : Ta;
    const Ri = R / Ie;
    let Cs = SDS / Ri, govern = 'SDS / (R/Ie)';
    const CsT = Tuse <= TL ? SD1 / (Tuse * Ri) : SD1 * TL / (Tuse * Tuse * Ri);
    if (CsT < Cs) { Cs = CsT; govern = Tuse <= TL ? 'SD1 / (T·R/Ie)' : 'SD1·TL / (T²·R/Ie)'; }
    const Csmin = Math.max(0.044 * SDS * Ie, 0.01);
    if (Cs < Csmin) { Cs = Csmin; govern = Csmin === 0.01 ? '0.01 minimum' : '0.044·SDS·Ie minimum'; }
    const V = Cs * Wt;
    const c = colors();
    const W = 760, H = 400, pad = { l: 60, r: 20, t: 26, b: 44 }, gw = W - pad.l - pad.r, gh = H - pad.t - pad.b;
    svgPlot(svg, W, H);
    const Tmax = Math.max(4, Math.ceil(Math.max(Tuse, Tmodel, TL * 0.6) * 1.2));
    const Smax = SDS * 1.15;
    const X = T => pad.l + T / Tmax * gw, Y = s => pad.t + gh - s / Smax * gh;
    // grid
    for (let i = 0; i <= 5; i++) { const s = Smax * i / 5; svg.appendChild(node('line', { x1: pad.l, y1: Y(s), x2: pad.l + gw, y2: Y(s), stroke: c.faint })); svg.appendChild(node('text', { x: pad.l - 8, y: Y(s) + 4, fill: c.text, 'font-size': 11, 'text-anchor': 'end' }, fmt(s, 2))); }
    for (let T = 0; T <= Tmax; T += Tmax <= 6 ? 0.5 : 1) { svg.appendChild(node('line', { x1: X(T), y1: pad.t, x2: X(T), y2: pad.t + gh, stroke: c.faint })); svg.appendChild(node('text', { x: X(T), y: pad.t + gh + 16, fill: c.text, 'font-size': 11, 'text-anchor': 'middle' }, fmt(T, 1))); }
    svg.appendChild(node('text', { x: pad.l + gw / 2, y: H - 8, fill: c.text, 'font-size': 12, 'text-anchor': 'middle' }, 'Period T (s)'));
    svg.appendChild(node('text', { x: 14, y: pad.t + gh / 2, fill: c.text, 'font-size': 12, 'text-anchor': 'middle', transform: `rotate(-90 14 ${pad.t + gh / 2})` }, 'Sa (g)'));
    // spectrum
    let d = ''; for (let i = 0; i <= 300; i++) { const T = Tmax * i / 300; d += (i ? ' L' : 'M') + X(T) + ',' + Y(Sa(T, SDS, SD1, TL)); }
    svg.appendChild(node('path', { d, fill: 'none', stroke: c.blue, 'stroke-width': 2.4 }));
    // design curve Sa/(R/Ie)
    let d2 = ''; for (let i = 0; i <= 300; i++) { const T = Tmax * i / 300; d2 += (i ? ' L' : 'M') + X(T) + ',' + Y(Math.max(Csmin, Sa(T, SDS, SD1, TL) / Ri)); }
    svg.appendChild(node('path', { d: d2, fill: 'none', stroke: c.teal, 'stroke-width': 2, 'stroke-dasharray': '6 4' }));
    // markers
    let mi = 0;
    const mark = (T, label, col) => { if (T > Tmax) return; const yl = pad.t + gh - 10 - 15 * (mi++); svg.appendChild(node('line', { x1: X(T), y1: pad.t, x2: X(T), y2: pad.t + gh, stroke: col, 'stroke-width': 1.2, 'stroke-dasharray': '3 3' })); svg.appendChild(node('text', { x: X(T) + 4, y: yl, fill: col, 'font-size': 11 }, label)); };
    mark(Ta, `Ta = ${fmt(Ta, 2)} s`, c.text);
    mark(Tcap, `Cu·Ta = ${fmt(Tcap, 2)} s`, c.orange);
    if (Tmodel > 0) mark(Tmodel, `T model = ${fmt(Tmodel, 2)} s`, c.ink);
    svg.appendChild(node('circle', { cx: X(Tuse), cy: Y(Sa(Tuse, SDS, SD1, TL)), r: 5, fill: c.blue }));
    svg.appendChild(node('circle', { cx: X(Tuse), cy: Y(Cs), r: 5, fill: c.teal }));
    // legend
    svg.appendChild(node('rect', { x: W - 290, y: pad.t + 6, width: 266, height: 44, fill: css('--surface') || '#fff', stroke: c.faint, rx: 6 }));
    svg.appendChild(node('line', { x1: W - 280, y1: pad.t + 20, x2: W - 252, y2: pad.t + 20, stroke: c.blue, 'stroke-width': 2.4 }));
    svg.appendChild(node('text', { x: W - 244, y: pad.t + 24, fill: c.text, 'font-size': 11 }, 'Design spectrum Sa (ASCE 7-22 §11.4.5)'));
    svg.appendChild(node('line', { x1: W - 280, y1: pad.t + 38, x2: W - 252, y2: pad.t + 38, stroke: c.teal, 'stroke-width': 2, 'stroke-dasharray': '6 4' }));
    svg.appendChild(node('text', { x: W - 244, y: pad.t + 42, fill: c.text, 'font-size': 11 }, 'Cs = Sa / (R/Ie), with the minimums'));
    const res = $('#spec-results'); res.innerHTML = '';
    [['Ta = Ct·hnˣ', fmt(Ta, 3) + ' s'], ['Cu · Ta', fmt(Tcap, 3) + ' s'], ['T used', fmt(Tuse, 3) + ' s'], ['Sa(T)', fmt(Sa(Tuse, SDS, SD1, TL), 3) + ' g'], ['Cs', fmt(Cs, 4)], ['Base shear V', fmt(V, 0) + ' kN']]
      .forEach(r => { const d = document.createElement('div'); d.innerHTML = `<b>${r[1]}</b><span>${r[0]}</span>`; res.appendChild(d); });
    $('#spec-note').textContent = `Cs governed by ${govern}. Cu = ${fmt(cu, 2)} from Table 12.8-1. Pondus scales its response-spectrum base shear up to this equivalent-lateral-force value, so the two never fall below it.`;
  }

  /* ------------------------------------------------------------------
     3. MODE SHAPES of a shear building: N storeys, storey mass m and
        stiffness k, optional taper. Eigenproblem K φ = ω² M φ solved by
        Jacobi rotations on M^-1/2 K M^-1/2; mass participation per mode;
        the chosen mode is animated.
     ------------------------------------------------------------------ */
  ['m-n', 'm-m', 'm-k', 'm-taper', 'm-h', 'm-mode'].forEach(id => { const el = $('#' + id); if (el) el.addEventListener('input', () => { modesDirty = true; drawModes(); }); });
  let modesDirty = true, modeData = null, modeAnimStart = performance.now();
  function jacobi(A) {
    const n = A.length; const V = Array.from({ length: n }, (_, i) => Array.from({ length: n }, (_, j) => i === j ? 1 : 0));
    for (let sweep = 0; sweep < 60; sweep++) {
      let off = 0; for (let i = 0; i < n; i++) for (let j = i + 1; j < n; j++) off += A[i][j] * A[i][j];
      if (off < 1e-18) break;
      for (let p = 0; p < n; p++) for (let q = p + 1; q < n; q++) {
        if (Math.abs(A[p][q]) < 1e-14) continue;
        const th = (A[q][q] - A[p][p]) / (2 * A[p][q]);
        const t = Math.sign(th || 1) / (Math.abs(th) + Math.sqrt(th * th + 1));
        const c = 1 / Math.sqrt(t * t + 1), s = t * c;
        for (let k = 0; k < n; k++) { const akp = A[k][p], akq = A[k][q]; A[k][p] = c * akp - s * akq; A[k][q] = s * akp + c * akq; }
        for (let k = 0; k < n; k++) { const apk = A[p][k], aqk = A[q][k]; A[p][k] = c * apk - s * aqk; A[q][k] = s * apk + c * aqk; }
        for (let k = 0; k < n; k++) { const vkp = V[k][p], vkq = V[k][q]; V[k][p] = c * vkp - s * vkq; V[k][q] = s * vkp + c * vkq; }
      }
    }
    return { lam: A.map((r, i) => r[i]), V };
  }
  function solveModes() {
    const N = Math.max(2, Math.min(14, Math.round(+$('#m-n').value))), m0 = +$('#m-m').value, k0 = +$('#m-k').value * 1000, taper = +$('#m-taper').value; // t, kN/m
    const masses = Array.from({ length: N }, (_, i) => m0 * (1 - taper * i / Math.max(1, N - 1)));
    const ks = Array.from({ length: N }, (_, i) => k0 * (1 - taper * i / Math.max(1, N - 1)));
    const K = Array.from({ length: N }, () => new Array(N).fill(0));
    for (let i = 0; i < N; i++) { K[i][i] += ks[i]; if (i + 1 < N) { K[i][i] += ks[i + 1]; K[i][i + 1] -= ks[i + 1]; K[i + 1][i] -= ks[i + 1]; } }
    // t·(kN/m) → ω² in rad²/s² : k [kN/m] / m [t] = (kN/m)/(1000 kg) = 1 N/(kg·m)·… → k/m directly in s⁻² when k in kN/m and m in t
    const A = K.map((r, i) => r.map((v, j) => v / Math.sqrt(masses[i] * masses[j])));
    const { lam, V } = jacobi(A.map(r => r.slice()));
    const order = lam.map((l, i) => i).sort((a, b) => lam[a] - lam[b]);
    const total = masses.reduce((s, v) => s + v, 0);
    const modes = order.map(i => {
      const phi = masses.map((mi, j) => V[j][i] / Math.sqrt(mi));
      const top = phi[N - 1]; const norm = phi.map(v => v / (top || 1));
      const Ln = norm.reduce((s, v, j) => s + masses[j] * v, 0), Mn = norm.reduce((s, v, j) => s + masses[j] * v * v, 0);
      const w = Math.sqrt(Math.max(lam[i], 1e-12));
      return { T: 2 * Math.PI / w, f: w / (2 * Math.PI), phi: norm, gamma: Ln / Mn, part: Ln * Ln / Mn / total };
    });
    return { N, modes, masses, total };
  }
  function drawModes(now) {
    const svg = $('#mode-plot'); if (!svg || !$('#panel-modes').classList.contains('active')) return;
    if (modesDirty || !modeData) { modeData = solveModes(); modesDirty = false; renderModeTable(); }
    const { N, modes } = modeData;
    const sel = Math.min(modes.length, Math.max(1, Math.round(+$('#m-mode').value))) - 1;
    $('#m-mode').max = N;
    const c = colors();
    const W = 760, H = 420, pad = 30;
    svgPlot(svg, W, H);
    const hs = +$('#m-h').value; const Htot = hs * N;
    const sy = z => H - pad - z / Htot * (H - 2 * pad);
    const amp = 60;
    const tsec = ((now || performance.now()) - modeAnimStart) / 1000;
    // draw three panels: selected mode animated large on the left, first three static on the right
    const cx = 200;
    const phase = Math.sin(2 * Math.PI * tsec / Math.max(0.6, Math.min(3, modes[sel].T)));
    // ground
    svg.appendChild(node('line', { x1: 40, y1: sy(0), x2: W - 40, y2: sy(0), stroke: c.ink, 'stroke-width': 2 }));
    const shape = (x0, mode, scale, animate, col, width) => {
      let d = `M${x0},${sy(0)}`;
      const pts = [[x0, sy(0)]];
      mode.phi.forEach((v, i) => { const x = x0 + v * scale * (animate ? phase : 1); pts.push([x, sy((i + 1) * hs)]); d += ` L${x},${sy((i + 1) * hs)}`; });
      // undeformed
      svg.appendChild(node('line', { x1: x0, y1: sy(0), x2: x0, y2: sy(Htot), stroke: c.faint, 'stroke-dasharray': '4 4' }));
      svg.appendChild(node('path', { d, fill: 'none', stroke: col, 'stroke-width': width, 'stroke-linejoin': 'round' }));
      pts.slice(1).forEach(p => { svg.appendChild(node('line', { x1: p[0] - 22, y1: p[1], x2: p[0] + 22, y2: p[1], stroke: col, 'stroke-width': width * 1.6, 'stroke-linecap': 'round' })); });
    };
    shape(cx, modes[sel], amp, true, c.blue, 2.4);
    svg.appendChild(node('text', { x: 40, y: H - 8, fill: c.ink, 'font-size': 13, 'font-weight': 700 }, `Mode ${sel + 1}: T = ${fmt(modes[sel].T, 3)} s, f = ${fmt(modes[sel].f, 2)} Hz, ${fmt(modes[sel].part * 100, 1)} % of the mass takes part`));
    svg.appendChild(node('text', { x: cx, y: 20, fill: c.text, 'font-size': 12, 'text-anchor': 'middle' }, `Mode ${sel + 1}, animated`));
    const small = Math.min(3, N);
    for (let i = 0; i < small; i++) {
      const x0 = 400 + i * 120;
      shape(x0, modes[i], 30, false, i === sel ? c.blue : c.teal, 1.8);
      svg.appendChild(node('text', { x: x0, y: 20, fill: c.text, 'font-size': 12, 'text-anchor': 'middle' }, `Mode ${i + 1} · ${fmt(modes[i].T, 2)} s`));
    }
  }
  // one animation loop for the mode shapes; it draws only while that panel is open
  if (!matchMedia('(prefers-reduced-motion: reduce)').matches) {
    const tick = now => { if ($('#panel-modes').classList.contains('active')) drawModes(now); requestAnimationFrame(tick); };
    requestAnimationFrame(tick);
  }
  function renderModeTable() {
    const tb = $('#mode-table tbody'); if (!tb) return; tb.innerHTML = '';
    let cum = 0;
    modeData.modes.slice(0, 6).forEach((m, i) => {
      cum += m.part;
      const tr = document.createElement('tr');
      tr.innerHTML = `<td>${i + 1}</td><td class="num">${fmt(m.T, 3)}</td><td class="num">${fmt(m.f, 2)}</td><td class="num">${fmt(m.part * 100, 1)} %</td><td class="num">${fmt(cum * 100, 1)} %</td>`;
      tb.appendChild(tr);
    });
    const need = modeData.modes.findIndex((_, i) => modeData.modes.slice(0, i + 1).reduce((s, m) => s + m.part, 0) >= 0.9) + 1;
    $('#mode-note').textContent = `${need} mode${need > 1 ? 's' : ''} reach 90 % of the mass, the target ASCE 7-22 §12.9.1.1 sets and the point where Pondus stops adding modes. Seismic weight ${fmt(modeData.total * 9.81, 0)} kN.`;
  }

  // Initial draw
  redrawAll();
  addEventListener('resize', () => { drawBeam(); drawSpectrum(); });
})();
