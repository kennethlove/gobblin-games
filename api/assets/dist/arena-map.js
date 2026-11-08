// iak: client-side arena map renderer.
// The server ships MapState JSON (areas + tokens); this draws it as
// layered SVG and re-renders on SSE-driven refetches, animating goblin
// movement Foundry-style: the token slides from its old wedge while a
// team-coloured trail fades behind it.
//
// Owns only the inside of .map-container. broadcast.js owns the svg's
// transform (zoom/pan) — the svg element is created once and mutated in
// place, never replaced, so its transform and broadcast.js's reference
// both survive re-renders.
(function () {
  "use strict";

  var NS = "http://www.w3.org/2000/svg";
  var SIZE = 52; // hex circumradius — matches the legacy map scale
  var CLUSTER_BELOW = 0.8; // zoomed out: collapse a hex's goblins to a count badge
  var MOVE_MS = 600;
  var TRAIL_MS = 2000;
  var reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  // Content palette: BaseTerrain variant name -> fill (moved out of the
  // server's terrain_color; values follow the old map exactly).
  var TERRAIN = {
    Forest: "#2d5a27",
    Desert: "#c4a35a",
    Tundra: "#8fa8b8",
    Wetlands: "#3a7a6e",
    Mountains: "#6b7280",
    UrbanRuins: "#78716c",
    Jungle: "#1a6b3c",
    Grasslands: "#6b8e3a",
    Badlands: "#8b6914",
    Highlands: "#5b7553",
    Geothermal: "#b45309",
    Clearing: "#4a6741"
  };

  var state = null; // last MapState
  var prevPos = null; // token id -> {x, y, area} before the last render
  var scale = window.__arenaScale || 1;
  var layers = { terrain: true, tokens: true, hazards: true, items: true, trails: true };

  function el(name, attrs, text) {
    var node = document.createElementNS(NS, name);
    for (var k in attrs) node.setAttribute(k, attrs[k]);
    if (text != null) node.textContent = text;
    return node;
  }

  // Axial -> pixel, same formula as areas::hex::Axial::to_pixel.
  function toPixel(q, r) {
    return {
      x: SIZE * (Math.sqrt(3) * q + (Math.sqrt(3) / 2) * r),
      y: SIZE * 1.5 * r
    };
  }

  // Vertex i of a hex; matches the legacy hex_corners angles (-90 + 60i).
  function vertex(cx, cy, i) {
    var a = ((-90 + 60 * i) * Math.PI) / 180;
    return [cx + SIZE * Math.cos(a), cy + SIZE * Math.sin(a)];
  }

  // Wedge i sits on the hex edge in axial direction i (Axial::neighbors
  // DIRS order NE,E,SE,SW,W,NW = -60+60i degrees — same order the server
  // uses for sub_tiles and cell_hazards). Its centroid is 2/3 of the way
  // from the apex (hex center) to the base midpoint (apothem out).
  function centroid(cx, cy, i) {
    var a = ((-60 + 60 * i) * Math.PI) / 180;
    var d = (2 / 3) * (Math.sqrt(3) / 2) * SIZE;
    return [cx + d * Math.cos(a), cy + d * Math.sin(a)];
  }

  function wedgePath(cx, cy, i) {
    var v1 = vertex(cx, cy, i);
    var v2 = vertex(cx, cy, i + 1);
    return (
      "M" + cx.toFixed(1) + "," + cy.toFixed(1) +
      " L" + v1[0].toFixed(1) + "," + v1[1].toFixed(1) +
      " L" + v2[0].toFixed(1) + "," + v2[1].toFixed(1) + " Z"
    );
  }

  function hexTip(a) {
    var bits = [a.name, a.terrain];
    a.hazards.forEach(function (h) {
      bits.push(h.event.toUpperCase() + " T" + (h.sub + 1) + " · " + h.blocks_left + " BLK");
    });
    if (a.items > 0) bits.push(a.items + " items");
    return bits.join(" — ");
  }

  function drawArea(a, gTerrain, gHazards, gItems) {
    var c = toPixel(a.q, a.r);
    var cx = c.x;
    var cy = c.y;

    for (var i = 0; i < 6; i++) {
      var sub = a.sub_tiles[i];
      var terrain = sub ? sub.terrain : a.terrain;
      var wedge = el("path", {
        d: wedgePath(cx, cy, i),
        fill: TERRAIN[terrain] || TERRAIN.Clearing,
        "fill-opacity": "0.85",
        class: "map-wedge"
      });
      wedge.appendChild(
        el("title", {}, terrain + (sub && sub.feature ? " — " + sub.feature : ""))
      );
      gTerrain.appendChild(wedge);
      if (sub && sub.feature) {
        var ct = centroid(cx, cy, i);
        gTerrain.appendChild(
          el("text", { x: ct[0].toFixed(1), y: ct[1].toFixed(1), class: "map-feature" }, sub.feature)
        );
      }
    }

    var pts = [];
    for (var v = 0; v < 6; v++) {
      var p = vertex(cx, cy, v);
      pts.push(p[0].toFixed(1) + "," + p[1].toFixed(1));
    }
    var outline = el("polygon", {
      points: pts.join(" "),
      fill: "none",
      stroke: a.open ? "var(--broad-border-strong)" : "var(--danger)",
      "stroke-width": "2",
      class: "map-hex"
    });
    if (!a.open) outline.setAttribute("stroke-dasharray", "5 3");
    outline.appendChild(el("title", {}, hexTip(a)));
    gTerrain.appendChild(outline);

    gTerrain.appendChild(
      el("text", { x: cx.toFixed(1), y: cy.toFixed(1), class: "map-name" }, a.name)
    );

    a.hazards.forEach(function (h) {
      if (h.sub > 5) return;
      var hp = el("path", { d: wedgePath(cx, cy, h.sub), class: "map-hazard" });
      hp.appendChild(
        el(
          "title",
          {},
          h.event.toUpperCase() + " · T" + (h.sub + 1) + " · " + h.blocks_left + " BLK"
        )
      );
      gHazards.appendChild(hp);
    });

    if (a.items > 0) {
      var v1 = vertex(cx, cy, 1);
      var badge = el("g", {
        transform: "translate(" + v1[0].toFixed(1) + "," + v1[1].toFixed(1) + ")",
        class: "map-items"
      });
      badge.appendChild(el("circle", { r: 8, class: "map-items-dot" }));
      badge.appendChild(el("text", { y: 3, class: "map-items-count" }, String(a.items)));
      badge.appendChild(el("title", {}, a.items + " items in " + a.name));
      gItems.appendChild(badge);
    }
  }

  // Returns token id -> {x, y, area} for movement diffing.
  function drawTokens(gTokens, areaIndex) {
    var positions = {};
    var byArea = {};
    state.tokens.forEach(function (t) {
      (byArea[t.area] = byArea[t.area] || []).push(t);
    });

    Object.keys(byArea).forEach(function (areaId) {
      var area = areaIndex[areaId];
      if (!area) return; // token in a hex the payload doesn't carry
      var c = toPixel(area.q, area.r);
      var list = byArea[areaId];
      var cluster = scale < CLUSTER_BELOW && list.length > 1;

      if (cluster) {
        var badge = el("g", {
          transform: "translate(" + c.x.toFixed(1) + "," + c.y.toFixed(1) + ")",
          class: "map-cluster"
        });
        badge.appendChild(el("circle", { r: 12, class: "map-cluster-dot" }));
        badge.appendChild(el("text", { y: 4, class: "map-cluster-count" }, String(list.length)));
        badge.appendChild(
          el("title", {}, list.length + " goblins in " + area.name)
        );
        gTokens.appendChild(badge);
        list.forEach(function (t) {
          positions[t.id] = { x: c.x, y: c.y, area: t.area };
        });
        return;
      }

      var n = list.length;
      list.forEach(function (t, i) {
        var x;
        var y;
        if (n === 1) {
          x = c.x;
          y = c.y;
        } else {
          var ang = (2 * Math.PI * i) / n - Math.PI / 2;
          x = c.x + 16 * Math.cos(ang);
          y = c.y + 16 * Math.sin(ang);
        }
        var g = el("g", {
          transform: "translate(" + x.toFixed(1) + "," + y.toFixed(1) + ")",
          class: "map-token" + (t.alive ? "" : " map-token-dead"),
          "data-char-id": t.id
        });
        var inner = el("g", { class: "map-token-inner" });
        var r = t.alive ? 11 : 8;
        inner.appendChild(
          el("circle", {
            r: r,
            fill: t.team_hex,
            stroke: "var(--broad-bg)",
            "stroke-width": "1.5"
          })
        );
        inner.appendChild(
          el(
            "text",
            { y: 3.5, class: "map-token-label" },
            t.name.charAt(0).toUpperCase()
          )
        );
        inner.appendChild(el("title", {}, t.name + (t.alive ? "" : " (fallen)")));
        // Avatar (fsx) clipped to the disc, on top: a broken URL just
        // leaves the team circle + initial beneath.
        if (t.avatar) {
          inner.appendChild(
            el("image", {
              class: "map-token-avatar",
              x: -r,
              y: -r,
              width: 2 * r,
              height: 2 * r,
              preserveAspectRatio: "xMidYMid slice",
              "clip-path": "url(#map-token-clip-" + (t.alive ? "alive" : "dead") + ")",
              href: t.avatar
            })
          );
        }
        g.appendChild(inner);
        gTokens.appendChild(g);
        positions[t.id] = { x: x, y: y, area: t.area };
      });
    });
    return positions;
  }

  function animateMoves(prev, cur, gTrails, areaIndex) {
    var tokenById = {};
    state.tokens.forEach(function (t) {
      tokenById[t.id] = t;
    });

    Object.keys(cur).forEach(function (id) {
      var from = prev[id];
      var to = cur[id];
      if (!from || (from.x === to.x && from.y === to.y)) return;

      // Trail: old wedge centroid -> (hex-boundary bend when crossing
      // hexes: the midpoint of the two hex centers is the shared edge's
      // midpoint) -> new wedge centroid.
      var d = "M" + from.x.toFixed(1) + "," + from.y.toFixed(1);
      if (from.area !== to.area) {
        var fa = areaIndex[from.area];
        var ta = areaIndex[to.area];
        if (fa && ta) {
          var fc = toPixel(fa.q, fa.r);
          var tc = toPixel(ta.q, ta.r);
          d += " L" + ((fc.x + tc.x) / 2).toFixed(1) + "," + ((fc.y + tc.y) / 2).toFixed(1);
        }
      }
      d += " L" + to.x.toFixed(1) + "," + to.y.toFixed(1);

      var token = tokenById[id];
      if (reduced) return;
      var trail = el("path", {
        d: d,
        class: "map-trail",
        stroke: token ? token.team_hex : "var(--broad-accent)"
      });
      gTrails.appendChild(trail);
      window.setTimeout(function () {
        if (trail.parentNode) trail.parentNode.removeChild(trail);
      }, TRAIL_MS + 100);
      var node = document.querySelector(
        '#arenaMap .map-token[data-char-id="' + id + '"] .map-token-inner'
      );
      if (!node || !node.animate) return;
      var dx0 = from.x - to.x;
      var dy0 = from.y - to.y;
      var frames = [
        { transform: "translate(" + dx0.toFixed(1) + "px," + dy0.toFixed(1) + "px)" },
        { transform: "translate(0px,0px)" }
      ];
      if (from.area !== to.area) {
        var fa2 = areaIndex[from.area];
        var ta2 = areaIndex[to.area];
        if (fa2 && ta2) {
          var fc2 = toPixel(fa2.q, fa2.r);
          var tc2 = toPixel(ta2.q, ta2.r);
          var bx = (fc2.x + tc2.x) / 2 - to.x;
          var by = (fc2.y + tc2.y) / 2 - to.y;
          frames.splice(1, 0, {
            transform: "translate(" + bx.toFixed(1) + "px," + by.toFixed(1) + "px)"
          });
        }
      }
      node.animate(frames, { duration: MOVE_MS, easing: "ease-out" });
    });
  }

  function applyLayerVis(g) {
    g.style.display = layers[g.getAttribute("data-layer")] ? "" : "none";
  }

  function render(animate) {
    var host = document.getElementById("arenaMap");
    if (!host || !state) return;

    var svg = host.querySelector("svg.hex-map");
    if (!svg) {
      svg = el("svg", {
        "class": "hex-map",
        xmlns: NS,
        role: "img",
        "aria-label": "Arena map"
      });
      host.appendChild(svg);
    }
    while (svg.firstChild) svg.removeChild(svg.firstChild);

    // Shared clips for avatar discs (fsx): one size alive, one fallen.
    var defs = el("defs", {});
    defs.appendChild(
      el("clipPath", { id: "map-token-clip-alive" }, null)
    );
    defs.firstChild.appendChild(el("circle", { r: 11 }));
    defs.appendChild(
      el("clipPath", { id: "map-token-clip-dead" }, null)
    );
    defs.lastChild.appendChild(el("circle", { r: 8 }));
    svg.appendChild(defs);

    var minx = Infinity;
    var miny = Infinity;
    var maxx = -Infinity;
    var maxy = -Infinity;
    var areaIndex = {};
    state.areas.forEach(function (a) {
      areaIndex[a.id] = a;
      var c = toPixel(a.q, a.r);
      minx = Math.min(minx, c.x - SIZE);
      miny = Math.min(miny, c.y - SIZE);
      maxx = Math.max(maxx, c.x + SIZE);
      maxy = Math.max(maxy, c.y + SIZE);
    });
    if (minx === Infinity) {
      minx = -SIZE;
      miny = -SIZE;
      maxx = SIZE;
      maxy = SIZE;
    }
    var pad = 28;
    svg.setAttribute(
      "viewBox",
      (minx - pad).toFixed(0) +
        " " +
        (miny - pad).toFixed(0) +
        " " +
        (maxx - minx + 2 * pad).toFixed(0) +
        " " +
        (maxy - miny + 2 * pad).toFixed(0)
    );

    var gTerrain = el("g", { "data-layer": "terrain" });
    var gHazards = el("g", { "data-layer": "hazards" });
    var gItems = el("g", { "data-layer": "items" });
    var gTrails = el("g", { "data-layer": "trails" });
    var gTokens = el("g", { "data-layer": "tokens" });

    state.areas.forEach(function (a) {
      drawArea(a, gTerrain, gHazards, gItems);
    });
    var positions = drawTokens(gTokens, areaIndex);

    [gTerrain, gHazards, gItems, gTrails, gTokens].forEach(function (g) {
      applyLayerVis(g);
      svg.appendChild(g);
    });

    if (animate && prevPos) animateMoves(prevPos, positions, gTrails, areaIndex);
    prevPos = positions;
  }

  // ── Live refetch: the hidden #mapLive div hits /map-state on SSE ──
  document.addEventListener("htmx:afterRequest", function (e) {
    if (!e.detail || !e.detail.xhr || !e.target || e.target.id !== "mapLive") return;
    try {
      var next = JSON.parse(e.detail.xhr.responseText);
      if (next && next.areas) {
        state = next;
        render(true);
      }
    } catch (err) {
      // Malformed payload: keep the last good render.
    }
  });

  // ── Zoom: broadcast.js dispatches arena:zoom on every scale change ──
  window.addEventListener("arena:zoom", function (e) {
    scale = (e.detail && e.detail.scale) || 1;
    render(false);
  });

  // ── Layer toggles ──
  document.querySelectorAll(".layer-btn").forEach(function (btn) {
    btn.addEventListener("click", function () {
      var name = btn.getAttribute("data-layer");
      layers[name] = !layers[name];
      btn.setAttribute("aria-pressed", String(layers[name]));
      var g = document.querySelector('#arenaMap svg [data-layer="' + name + '"]');
      if (g) g.style.display = layers[name] ? "" : "none";
    });
  });

  // ── Initial paint from the server-embedded state ──
  var stateEl = document.getElementById("arenaState");
  if (stateEl) {
    try {
      state = JSON.parse(stateEl.textContent);
    } catch (err) {
      state = null;
    }
  }
  render(false);
})();
