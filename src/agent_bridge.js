// ─────────────────────────────────────────────────────────────────────────────
//  flutterff-rs In-Page GUI Agent & Inspector Bridge (Inspired by alibaba/page-agent)
//  - Captures console.log/warn/error/info/debug -> flutterLog
//  - Auto-activates Flutter Web Semantics (<flt-semantics-placeholder>)
//  - Indexes interactive Flutter Semantics & DOM widgets ([0], [1], [2]...)
//  - Adapts pointer visual & events:
//      * Desktop mode ("cursor"): Animated Electric Cyan SVG arrow cursor + mouse events
//      * Mobile/Tablet mode ("touch"): Glowing Touch Dot (●) + ripple wave + touch events
//  - Provides Set-of-Mark ([0], [1]...) bounding-box inspector overlay
// ─────────────────────────────────────────────────────────────────────────────
(function () {
  if (window.__flutterff) return;

  var MCP_PREFIX = "__FF_MCP__:";

  function postToHost(msg) {
    try {
      window.webkit.messageHandlers.flutterLog.postMessage(msg);
    } catch (_) {}
  }

  function sendMcpReply(reqId, resultObj) {
    postToHost(MCP_PREFIX + JSON.stringify({ req_id: reqId, result: resultObj }));
  }

  // 1. Console, Uncaught Error & Network Bridge (Obscura-style observability)
  ["log", "warn", "error", "info", "debug"].forEach(function (level) {
    var orig = console[level];
    console[level] = function () {
      var msg = Array.prototype.slice.call(arguments).join(" ");
      if (msg.indexOf(MCP_PREFIX) !== 0) {
        postToHost(msg);
      }
      orig.apply(console, arguments);
    };
  });

  window.addEventListener("error", function (ev) {
    var detail = ev && (ev.message || (ev.error && ev.error.stack) || "Runtime error");
    postToHost("flutter: error: " + detail);
  });

  window.addEventListener("unhandledrejection", function (ev) {
    var reason = ev && ev.reason ? (ev.reason.stack || ev.reason.message || String(ev.reason)) : "Promise rejection";
    postToHost("flutter: unhandled: " + reason);
  });

  var MAX_NET_HISTORY = 100;
  var networkLog = [];

  function pushNetEntry(entry) {
    if (networkLog.length >= MAX_NET_HISTORY) {
      networkLog.shift();
    }
    networkLog.push(entry);
  }

  if (typeof window.fetch === "function") {
    var origFetch = window.fetch.bind(window);
    window.fetch = function (input, init) {
      var method = (init && init.method) || (input && input.method) || "GET";
      var url = typeof input === "string" ? input : (input && input.url) || String(input);
      var t0 = Date.now();
      return origFetch(input, init).then(
        function (resp) {
          pushNetEntry({
            type: "fetch",
            method: String(method).toUpperCase(),
            url: url,
            status: resp.status,
            ok: resp.ok,
            duration_ms: Date.now() - t0
          });
          return resp;
        },
        function (err) {
          pushNetEntry({
            type: "fetch",
            method: String(method).toUpperCase(),
            url: url,
            status: 0,
            ok: false,
            duration_ms: Date.now() - t0,
            error: String(err)
          });
          throw err;
        }
      );
    };
  }

  if (typeof window.XMLHttpRequest === "function") {
    var origOpen = window.XMLHttpRequest.prototype.open;
    var origSend = window.XMLHttpRequest.prototype.send;
    window.XMLHttpRequest.prototype.open = function (method, url) {
      this.__ff_method = String(method || "GET").toUpperCase();
      this.__ff_url = String(url || "");
      return origOpen.apply(this, arguments);
    };
    window.XMLHttpRequest.prototype.send = function () {
      var xhr = this;
      var t0 = Date.now();
      xhr.addEventListener("loadend", function () {
        pushNetEntry({
          type: "xhr",
          method: xhr.__ff_method || "GET",
          url: xhr.__ff_url || "",
          status: xhr.status || 0,
          ok: xhr.status >= 200 && xhr.status < 400,
          duration_ms: Date.now() - t0
        });
      });
      return origSend.apply(this, arguments);
    };
  }

  // 2. State & Configuration
  var state = {
    pointerMode: "auto", // "auto" | "cursor" | "touch"
    inspectVisible: false,
    selectorMap: [],
    prevSnapshot: null,
    cursorX: 180,
    cursorY: 320,
    maskRoot: null,
    cursorEl: null,
    dotEl: null,
    boxesEl: null,
    hideDotTimer: null
  };

  function effectivePointerMode() {
    if (state.pointerMode === "cursor" || state.pointerMode === "touch") {
      return state.pointerMode;
    }
    return window.innerWidth >= 1024 ? "cursor" : "touch";
  }

  // 3. Visual Overlay (Desktop Cursor + Mobile Touch Dot + Inspector Boxes)
  function ensureMask() {
    if (state.maskRoot && document.body && document.body.contains(state.maskRoot)) {
      return;
    }
    if (!document.body) return;

    var style = document.getElementById("__flutterff_style");
    if (!style) {
      style = document.createElement("style");
      style.id = "__flutterff_style";
      style.textContent = [
        "#__flutterff_mask { position: fixed; inset: 0; pointer-events: none; z-index: 2147483647; overflow: hidden; font-family: monospace; }",
        ".__ff_cursor { position: absolute; top: 0; left: 0; width: 24px; height: 24px; margin-left: -5px; margin-top: -1px; pointer-events: none; transition: transform 0.16s cubic-bezier(0.22, 1, 0.36, 1), opacity 0.18s ease; filter: drop-shadow(0 2px 4px rgba(0, 0, 0, 0.45)); opacity: 0; transform-origin: 5px 1px; }",
        ".__ff_touch_dot { position: absolute; top: 0; left: 0; width: 12px; height: 12px; margin-left: -6px; margin-top: -6px; border-radius: 50%; background: #FF4500; box-shadow: 0 1px 4px rgba(255, 69, 0, 0.55); pointer-events: none; transition: transform 0.15s cubic-bezier(0.22, 1, 0.36, 1), opacity 0.18s ease; opacity: 0; }",
        ".__ff_box { position: absolute; border: 1.5px dashed #00E5FF; background: rgba(0, 229, 255, 0.06); border-radius: 4px; pointer-events: none; box-sizing: border-box; }",
        ".__ff_box_input { border-color: #50FA7B; background: rgba(80, 250, 123, 0.06); }",
        ".__ff_box_overflow { border-color: #F38BA8; background: rgba(243, 139, 168, 0.14); }",
        ".__ff_badge { position: absolute; top: -1px; left: -1px; background: #1E1E2E; color: #00E5FF; border: 1px solid #00E5FF; border-radius: 3px; padding: 0 4px; font-size: 10px; font-weight: 700; line-height: 14px; white-space: nowrap; }",
        ".__ff_box_input .__ff_badge { color: #50FA7B; border-color: #50FA7B; }",
        ".__ff_box_overflow .__ff_badge { color: #F38BA8; border-color: #F38BA8; }"
      ].join("\n");
      document.head.appendChild(style);
    }

    var root = document.createElement("div");
    root.id = "__flutterff_mask";
    root.setAttribute("data-flutterff-ignore", "true");

    var boxes = document.createElement("div");
    boxes.id = "__flutterff_boxes";
    root.appendChild(boxes);

    // Bibata-Modern-Ice shaped Desktop Cursor in Red-Orange (#FF4500)
    var cursor = document.createElement("div");
    cursor.className = "__ff_cursor";
    cursor.innerHTML =
      '<svg width="24" height="24" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">' +
      '<path d="M5.5 2.2C5.5 1.3 6.6 0.9 7.3 1.5L19.3 12.8C20.1 13.5 19.7 14.9 18.6 15.0L12.6 15.7C12.1 15.8 11.7 16.1 11.4 16.5L8.3 20.9C7.6 21.8 6.1 21.4 5.9 20.3C5.6 18.8 5.5 16.5 5.5 14.0V2.2Z" fill="#FF4500" stroke="#FFFFFF" stroke-width="1.5" stroke-linejoin="round"/>' +
      "</svg>";
    root.appendChild(cursor);

    // Small Red-Orange Mobile Touch Dot (no outer ring)
    var dot = document.createElement("div");
    dot.className = "__ff_touch_dot";
    root.appendChild(dot);

    document.body.appendChild(root);
    state.maskRoot = root;
    state.boxesEl = boxes;
    state.cursorEl = cursor;
    state.dotEl = dot;
  }

  function showPointerInteraction(x, y, endX, endY) {
    ensureMask();
    if (!state.maskRoot) return;

    var mode = effectivePointerMode();
    state.cursorX = x;
    state.cursorY = y;

    if (mode === "cursor") {
      if (state.dotEl) state.dotEl.style.opacity = "0";
      if (state.cursorEl) {
        state.cursorEl.style.opacity = "1";
        state.cursorEl.style.transform = "translate(" + x + "px, " + y + "px) scale(0.9)";
        setTimeout(function () {
          if (!state.cursorEl) return;
          var tx = typeof endX === "number" ? endX : x;
          var ty = typeof endY === "number" ? endY : y;
          state.cursorEl.style.transform = "translate(" + tx + "px, " + ty + "px) scale(1)";
        }, 90);
      }
    } else {
      if (state.cursorEl) state.cursorEl.style.opacity = "0";
      if (state.dotEl) {
        if (state.hideDotTimer) clearTimeout(state.hideDotTimer);
        state.dotEl.style.opacity = "1";
        state.dotEl.style.transform = "translate(" + x + "px, " + y + "px) scale(1)";
        if (typeof endX === "number" && typeof endY === "number") {
          setTimeout(function () {
            if (state.dotEl) {
              state.dotEl.style.transform = "translate(" + endX + "px, " + endY + "px) scale(0.9)";
            }
          }, 50);
        }
        state.hideDotTimer = setTimeout(function () {
          if (state.dotEl) state.dotEl.style.opacity = "0";
        }, 550);
      }
    }
  }

  // 4. Deep Shadow DOM Traversal & Flutter Semantics Auto-Activation
  function queryAllDeep(root, predicate, out) {
    out = out || [];
    if (!root) return out;
    var children = root.children || root.childNodes || [];
    for (var i = 0; i < children.length; i++) {
      var el = children[i];
      if (!el || el.nodeType !== 1) continue;
      if (el.id === "__flutterff_mask" || el.getAttribute("data-flutterff-ignore")) continue;
      if (predicate(el)) out.push(el);
      if (el.shadowRoot) {
        queryAllDeep(el.shadowRoot, predicate, out);
      }
      queryAllDeep(el, predicate, out);
    }
    return out;
  }

  function ensureFlutterSemantics(callback) {
    var existing = queryAllDeep(document, function (el) {
      return el.tagName && el.tagName.toLowerCase() === "flt-semantics";
    });
    if (existing.length > 0) {
      callback(true);
      return;
    }

    var placeholders = queryAllDeep(document, function (el) {
      return el.tagName && el.tagName.toLowerCase() === "flt-semantics-placeholder";
    });

    if (placeholders.length > 0) {
      var ph = placeholders[0];
      try {
        ph.focus();
        ph.click();
        ph.dispatchEvent(new MouseEvent("mousedown", { bubbles: true, cancelable: true }));
        ph.dispatchEvent(new MouseEvent("mouseup", { bubbles: true, cancelable: true }));
        ph.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }));
      } catch (_) {}
      setTimeout(function () {
        var after = queryAllDeep(document, function (el) {
          return el.tagName && el.tagName.toLowerCase() === "flt-semantics";
        });
        callback(after.length > 0);
      }, 140);
      return;
    }

    callback(false);
  }

  var INTERACTIVE_ROLES = {
    button: true,
    checkbox: true,
    switch: true,
    radio: true,
    tab: true,
    menuitem: true,
    link: true,
    slider: true,
    textbox: true
  };

  var SCAN_TAGS = {
    "flt-semantics": true,
    input: true,
    textarea: true,
    select: true,
    button: true,
    a: true
  };

  function collapseWhitespace(str) {
    return (str || "").split(" ").filter(Boolean).join(" ");
  }

  function classifyElement(el) {
    var tag = (el.tagName || "").toLowerCase();
    var roleAttr = (el.getAttribute("role") || "").toLowerCase();
    var ariaLabel = el.getAttribute("aria-label") || "";
    var hasTappable = el.hasAttribute("flt-tappable") || el.getAttribute("tabindex") === "0";

    if (tag === "flt-semantics") {
      var childInput = el.querySelector("input, textarea");
      if (childInput) {
        return {
          role: "textfield",
          label: ariaLabel || childInput.getAttribute("aria-label") || childInput.placeholder || "",
          value: childInput.value || "",
          interactive: true,
          isInput: true,
          targetNode: childInput
        };
      }
      var isInteractive = hasTappable || Boolean(INTERACTIVE_ROLES[roleAttr]);
      if (!isInteractive && !ariaLabel) return null;

      return {
        role: roleAttr || (hasTappable ? "button" : "label"),
        label: ariaLabel || (el.innerText || "").trim(),
        value: el.getAttribute("aria-checked") || el.getAttribute("aria-valuenow") || "",
        interactive: isInteractive,
        isInput: roleAttr === "textbox",
        targetNode: el
      };
    }

    if (tag === "input" || tag === "textarea" || tag === "select") {
      if (el.closest && el.closest("flt-semantics")) return null;
      return {
        role: "textfield",
        label: ariaLabel || el.placeholder || el.name || tag,
        value: el.value || "",
        interactive: true,
        isInput: true,
        targetNode: el
      };
    }

    if (
      tag === "button" ||
      tag === "a" ||
      roleAttr === "button" ||
      roleAttr === "link" ||
      el.onclick != null
    ) {
      return {
        role: roleAttr || tag,
        label: (ariaLabel || el.innerText || el.textContent || "").trim().slice(0, 120),
        value: "",
        interactive: true,
        isInput: false,
        targetNode: el
      };
    }

    return null;
  }

  function buildUiTree(includeStatic) {
    var rawNodes = queryAllDeep(document.body || document.documentElement, function (el) {
      var tag = (el.tagName || "").toLowerCase();
      return Boolean(SCAN_TAGS[tag]) || el.hasAttribute("role") || el.hasAttribute("flt-tappable");
    });

    var items = [];
    var seenKeys = {};

    for (var i = 0; i < rawNodes.length; i++) {
      var el = rawNodes[i];
      var info = classifyElement(el);
      if (!info) continue;
      if (!includeStatic && !info.interactive && !info.label) continue;

      var rect = el.getBoundingClientRect();
      if (rect.width <= 2 || rect.height <= 2) continue;
      if (
        rect.bottom < 0 ||
        rect.right < 0 ||
        rect.top > window.innerHeight ||
        rect.left > window.innerWidth
      ) {
        continue;
      }

      var rx = Math.round(rect.left);
      var ry = Math.round(rect.top);
      var rw = Math.round(rect.width);
      var rh = Math.round(rect.height);
      var cx = Math.round(rect.left + rect.width / 2);
      var cy = Math.round(rect.top + rect.height / 2);

      var dedupKey = [info.role, info.label, rx, ry, rw, rh].join("|");
      if (seenKeys[dedupKey]) continue;
      seenKeys[dedupKey] = true;

      var idx = items.length;
      var isOverflow = rect.right > window.innerWidth + 2 || rect.left < -2;
      items.push({
        index: idx,
        role: info.role,
        label: info.label,
        value: info.value,
        interactive: info.interactive,
        isInput: info.isInput,
        overflow: isOverflow,
        x: rx,
        y: ry,
        width: rw,
        height: rh,
        center_x: cx,
        center_y: cy,
        node: info.targetNode,
        semNode: el
      });
    }

    state.selectorMap = items;
    if (state.inspectVisible) {
      renderInspectorBoxes();
    }
    return items;
  }

  function renderInspectorBoxes() {
    ensureMask();
    if (!state.boxesEl) return;
    state.boxesEl.innerHTML = "";
    if (!state.inspectVisible) return;

    for (var i = 0; i < state.selectorMap.length; i++) {
      var item = state.selectorMap[i];
      var box = document.createElement("div");
      box.className =
        "__ff_box" +
        (item.isInput ? " __ff_box_input" : "") +
        (item.overflow ? " __ff_box_overflow" : "");
      box.style.left = item.x + "px";
      box.style.top = item.y + "px";
      box.style.width = item.width + "px";
      box.style.height = item.height + "px";

      var badge = document.createElement("div");
      badge.className = "__ff_badge";
      var shortRole = item.role === "textfield" ? "input" : item.role;
      badge.textContent =
        "[" + item.index + "] " + shortRole + (item.overflow ? " OVERFLOW" : "");
      box.appendChild(badge);
      state.boxesEl.appendChild(box);
    }
  }

  function formatUiTreeText(items, semActive, maxChars) {
    var budget = typeof maxChars === "number" && maxChars > 0 ? maxChars : 4000;
    var mode = effectivePointerMode();
    var overflowCount = 0;
    for (var k = 0; k < items.length; k++) {
      if (items[k].overflow) overflowCount++;
    }
    var header =
      "Viewport: " +
      window.innerWidth +
      "x" +
      window.innerHeight +
      "px | Pointer: " +
      mode +
      (mode === "touch" ? " (mobile dot)" : " (desktop cursor)") +
      " | Semantics: " +
      (semActive ? "active" : "dom-only") +
      " | Elements: " +
      items.length +
      (overflowCount > 0 ? " | Overflows: " + overflowCount : "");

    if (items.length === 0) {
      return (
        header +
        "\n(No interactive semantics/DOM elements detected yet. If Flutter is still rendering on <canvas>, try tapping by coordinates or enabling semantics.)"
      );
    }

    var lines = [header];
    for (var i = 0; i < items.length; i++) {
      var it = items[i];
      var lbl = it.label ? ' "' + collapseWhitespace(it.label) + '"' : "";
      var val = it.value ? ' val="' + it.value + '"' : "";
      var ovr = it.overflow ? " [OVERFLOW]" : "";
      lines.push(
        "[" +
          it.index +
          "] [" +
          it.role +
          "]" +
          lbl +
          val +
          " @(" +
          it.center_x +
          ", " +
          it.center_y +
          ") " +
          it.width +
          "x" +
          it.height +
          ovr
      );
    }
    var out = lines.join("\n");
    return out.length > budget
      ? out.slice(0, budget) + "\n... (truncated to max_chars=" + budget + ")"
      : out;
  }

  function snapshotItemKey(it) {
    return it.role + "|" + collapseWhitespace(it.label || "");
  }

  function computeUiDiff(items) {
    var prev = state.prevSnapshot;
    var nextMap = {};
    var nextList = [];
    for (var i = 0; i < items.length; i++) {
      var it = items[i];
      var key = snapshotItemKey(it) + "#" + i;
      var baseKey = snapshotItemKey(it);
      var entry = {
        index: it.index,
        role: it.role,
        label: collapseWhitespace(it.label || ""),
        value: it.value || "",
        overflow: Boolean(it.overflow),
        center_x: it.center_x,
        center_y: it.center_y,
        baseKey: baseKey
      };
      nextMap[key] = entry;
      nextList.push(entry);
    }
    state.prevSnapshot = nextList;

    if (!prev) {
      return {
        has_baseline: false,
        added: nextList.length,
        changed: 0,
        removed: 0,
        summary: formatUiTreeText(items, true, 2000)
      };
    }

    var prevByBase = {};
    for (var p = 0; p < prev.length; p++) {
      var pe = prev[p];
      if (!prevByBase[pe.baseKey]) prevByBase[pe.baseKey] = [];
      prevByBase[pe.baseKey].push(pe);
    }

    var addedLines = [];
    var changedLines = [];
    var matchedPrev = {};

    for (var j = 0; j < nextList.length; j++) {
      var cur = nextList[j];
      var candidates = prevByBase[cur.baseKey];
      var matched = null;
      if (candidates) {
        for (var c = 0; c < candidates.length; c++) {
          var cand = candidates[c];
          if (!matchedPrev[cand.index]) {
            matched = cand;
            matchedPrev[cand.index] = true;
            break;
          }
        }
      }
      var lbl = cur.label ? ' "' + cur.label + '"' : "";
      var val = cur.value ? ' val="' + cur.value + '"' : "";
      var ovr = cur.overflow ? " [OVERFLOW]" : "";
      if (!matched) {
        addedLines.push(
          "[+] [" +
            cur.index +
            "] [" +
            cur.role +
            "]" +
            lbl +
            val +
            " @(" +
            cur.center_x +
            ", " +
            cur.center_y +
            ")" +
            ovr
        );
      } else if (matched.value !== cur.value || matched.overflow !== cur.overflow) {
        changedLines.push(
          "[~] [" +
            cur.index +
            "] [" +
            cur.role +
            "]" +
            lbl +
            ' val="' +
            matched.value +
            '" -> "' +
            cur.value +
            '"' +
            ovr
        );
      }
    }

    var removedLines = [];
    for (var r = 0; r < prev.length; r++) {
      var oldIt = prev[r];
      if (!matchedPrev[oldIt.index]) {
        var rLbl = oldIt.label ? ' "' + oldIt.label + '"' : "";
        removedLines.push("[-] [" + oldIt.role + "]" + rLbl);
      }
    }

    var diffLines = [
      "UI Diff: +" +
        addedLines.length +
        " added, ~" +
        changedLines.length +
        " changed, -" +
        removedLines.length +
        " removed (total=" +
        nextList.length +
        ")"
    ]
      .concat(addedLines)
      .concat(changedLines)
      .concat(removedLines);

    return {
      has_baseline: true,
      added: addedLines.length,
      changed: changedLines.length,
      removed: removedLines.length,
      summary: diffLines.join("\n")
    };
  }

  function filterUiItems(items, query, roleFilter) {
    var q = typeof query === "string" ? query.trim().toLowerCase() : "";
    var r = typeof roleFilter === "string" ? roleFilter.trim().toLowerCase() : "";
    if (!q && !r) return items;
    var filtered = [];
    for (var i = 0; i < items.length; i++) {
      var it = items[i];
      var roleMatch = !r || (it.role || "").toLowerCase().indexOf(r) !== -1;
      var queryMatch =
        !q ||
        ((it.label || "") + " " + (it.value || "") + " " + (it.role || ""))
          .toLowerCase()
          .indexOf(q) !== -1;
      if (roleMatch && queryMatch) {
        filtered.push(it);
      }
    }
    return filtered;
  }

  // 5. Target Resolution (By Index, By Label/Text, or By Coordinates with Stale-Node Auto-Refresh)
  function findByText(needle) {
    for (var i = 0; i < state.selectorMap.length; i++) {
      var candidate = state.selectorMap[i];
      if (candidate.node && !candidate.node.isConnected) continue;
      var hay = ((candidate.label || "") + " " + (candidate.value || "")).toLowerCase();
      if (hay.indexOf(needle) !== -1) {
        return candidate;
      }
    }
    return null;
  }

  function resolveTarget(args) {
    if (state.selectorMap.length === 0) {
      buildUiTree(true);
    }

    if (typeof args.index === "number" && args.index >= 0) {
      var byIdx = state.selectorMap[args.index];
      if (!byIdx || (byIdx.node && !byIdx.node.isConnected)) {
        buildUiTree(true);
        byIdx = state.selectorMap[args.index];
      }
      if (!byIdx) {
        return { error: "Element index [" + args.index + "] not found in current UI tree" };
      }
      return {
        x: byIdx.center_x,
        y: byIdx.center_y,
        item: byIdx
      };
    }

    if (typeof args.text === "string" && args.text.length > 0) {
      var needle = args.text.toLowerCase();
      var found = findByText(needle);
      if (!found) {
        buildUiTree(true);
        found = findByText(needle);
      }
      if (!found) {
        return { error: 'No element matching text "' + args.text + '" found' };
      }
      return {
        x: found.center_x,
        y: found.center_y,
        item: found
      };
    }

    if (typeof args.x === "number" && typeof args.y === "number") {
      return {
        x: Math.round(args.x),
        y: Math.round(args.y),
        item: null
      };
    }

    return { error: "Provide 'index', 'text', or both 'x' and 'y' coordinates" };
  }

  function deepElementFromPoint(x, y) {
    var el = document.elementFromPoint(x, y);
    while (el && el.shadowRoot && el.shadowRoot.elementFromPoint) {
      var inner = el.shadowRoot.elementFromPoint(x, y);
      if (!inner || inner === el) break;
      el = inner;
    }
    return el || document.body;
  }

  function dispatchTapSequence(x, y, item) {
    var mode = effectivePointerMode();
    var pType = mode === "touch" ? "touch" : "mouse";
    var hitEl = (item && (item.node || item.semNode)) || deepElementFromPoint(x, y);

    var commonInit = {
      bubbles: true,
      cancelable: true,
      composed: true,
      view: window,
      clientX: x,
      clientY: y,
      screenX: x,
      screenY: y,
      button: 0,
      buttons: 1
    };

    var ptrInit = Object.assign({}, commonInit, {
      pointerId: 1,
      pointerType: pType,
      isPrimary: true,
      width: mode === "touch" ? 20 : 1,
      height: mode === "touch" ? 20 : 1,
      pressure: 0.5
    });

    try {
      if (mode === "cursor") {
        hitEl.dispatchEvent(new PointerEvent("pointermove", ptrInit));
        hitEl.dispatchEvent(new MouseEvent("mousemove", commonInit));
        hitEl.dispatchEvent(new PointerEvent("pointerover", ptrInit));
        hitEl.dispatchEvent(new MouseEvent("mouseover", commonInit));
      }

      hitEl.dispatchEvent(new PointerEvent("pointerdown", ptrInit));
      if (mode === "touch" && typeof TouchEvent !== "undefined" && typeof Touch !== "undefined") {
        try {
          var touchObj = new Touch({
            identifier: 1,
            target: hitEl,
            clientX: x,
            clientY: y,
            radiusX: 10,
            radiusY: 10,
            force: 0.5
          });
          hitEl.dispatchEvent(
            new TouchEvent("touchstart", {
              bubbles: true,
              cancelable: true,
              composed: true,
              touches: [touchObj],
              targetTouches: [touchObj],
              changedTouches: [touchObj]
            })
          );
          hitEl.dispatchEvent(
            new TouchEvent("touchend", {
              bubbles: true,
              cancelable: true,
              composed: true,
              touches: [],
              targetTouches: [],
              changedTouches: [touchObj]
            })
          );
        } catch (_) {}
      }
      hitEl.dispatchEvent(new MouseEvent("mousedown", commonInit));

      if (hitEl.focus) {
        hitEl.focus({ preventScroll: true });
      }

      var upInit = Object.assign({}, commonInit, { buttons: 0 });
      var ptrUpInit = Object.assign({}, ptrInit, { buttons: 0, pressure: 0 });
      hitEl.dispatchEvent(new PointerEvent("pointerup", ptrUpInit));
      hitEl.dispatchEvent(new MouseEvent("mouseup", upInit));
      hitEl.dispatchEvent(new MouseEvent("click", upInit));

      if (item && item.semNode && item.semNode !== hitEl && item.semNode.click) {
        item.semNode.click();
      }
    } catch (_) {}
  }

  function findActiveTextInput() {
    var active = document.activeElement;
    while (active && active.shadowRoot && active.shadowRoot.activeElement) {
      active = active.shadowRoot.activeElement;
    }
    if (
      active &&
      active.tagName &&
      (active.tagName.toLowerCase() === "input" || active.tagName.toLowerCase() === "textarea")
    ) {
      return active;
    }

    var allInputs = queryAllDeep(document, function (el) {
      var t = (el.tagName || "").toLowerCase();
      return t === "input" || t === "textarea";
    });
    return allInputs.length > 0 ? allInputs[allInputs.length - 1] : null;
  }

  function setNativeInputValue(inputEl, text, clearFirst) {
    var tag = (inputEl.tagName || "").toLowerCase();
    var proto =
      tag === "textarea" ? window.HTMLTextAreaElement.prototype : window.HTMLInputElement.prototype;
    var descriptor = Object.getOwnPropertyDescriptor(proto, "value");
    var nextVal = clearFirst ? text : (inputEl.value || "") + text;

    inputEl.focus({ preventScroll: true });
    if (descriptor && descriptor.set) {
      descriptor.set.call(inputEl, nextVal);
    } else {
      inputEl.value = nextVal;
    }

    inputEl.dispatchEvent(
      new InputEvent("input", {
        bubbles: true,
        cancelable: true,
        composed: true,
        data: text,
        inputType: "insertText"
      })
    );
    inputEl.dispatchEvent(new Event("change", { bubbles: true, composed: true }));
  }

  function dispatchKey(targetEl, keyName) {
    var codeMap = {
      Enter: 13,
      Escape: 27,
      Backspace: 8,
      Tab: 9,
      ArrowUp: 38,
      ArrowDown: 40,
      ArrowLeft: 37,
      ArrowRight: 39,
      " ": 32
    };
    var keyCode = codeMap[keyName] || keyName.charCodeAt(0) || 0;
    var opts = {
      key: keyName,
      code: keyName === " " ? "Space" : keyName,
      keyCode: keyCode,
      which: keyCode,
      bubbles: true,
      cancelable: true,
      composed: true
    };
    targetEl.dispatchEvent(new KeyboardEvent("keydown", opts));
    targetEl.dispatchEvent(new KeyboardEvent("keypress", opts));
    targetEl.dispatchEvent(new KeyboardEvent("keyup", opts));
  }

  // 6. Table-Driven Action Handlers & Public RPC Dispatcher
  var ACTION_HANDLERS = {
    ui_tree: function (reqId, payload) {
      ensureFlutterSemantics(function (semActive) {
        if (typeof payload.overlay === "boolean") {
          state.inspectVisible = payload.overlay;
        }
        var allItems = buildUiTree(Boolean(payload.include_static));
        renderInspectorBoxes();
        var diffInfo = computeUiDiff(allItems);
        var items = filterUiItems(allItems, payload.query, payload.role);
        var overflowCount = 0;
        var cleanItems = items.map(function (it) {
          if (it.overflow) overflowCount++;
          return {
            index: it.index,
            role: it.role,
            label: it.label,
            value: it.value,
            overflow: it.overflow,
            x: it.x,
            y: it.y,
            width: it.width,
            height: it.height,
            center_x: it.center_x,
            center_y: it.center_y
          };
        });
        var summaryText =
          payload.diff && diffInfo.has_baseline
            ? diffInfo.summary
            : formatUiTreeText(items, semActive, payload.max_chars);
        sendMcpReply(reqId, {
          ok: true,
          pointer_mode: effectivePointerMode(),
          semantics_active: semActive,
          count: cleanItems.length,
          total_elements: allItems.length,
          overflow_count: overflowCount,
          diff_added: diffInfo.added,
          diff_changed: diffInfo.changed,
          diff_removed: diffInfo.removed,
          summary: summaryText,
          elements: cleanItems
        });
      });
    },

    fill_form: function (reqId, payload) {
      ensureFlutterSemantics(function () {
        var fields = Array.isArray(payload.fields) ? payload.fields : [];
        if (fields.length === 0) {
          sendMcpReply(reqId, { ok: false, error: "'fields' must be a non-empty array" });
          return;
        }
        computeUiDiff(buildUiTree(true));
        var filledCount = 0;
        var errors = [];
        var stepIdx = 0;

        function nextField() {
          if (stepIdx >= fields.length) {
            if (typeof payload.submit_index === "number" || typeof payload.submit_text === "string") {
              var sub = resolveTarget({
                index: payload.submit_index,
                text: payload.submit_text
              });
              if (!sub.error) {
                showPointerInteraction(sub.x, sub.y);
                dispatchTapSequence(sub.x, sub.y, sub.item);
              } else {
                errors.push(sub.error);
              }
            }
            setTimeout(function () {
              var postDiff = computeUiDiff(buildUiTree(true));
              sendMcpReply(reqId, {
                ok: errors.length === 0,
                action: "fill_form",
                filled: filledCount,
                submitted: Boolean(
                  typeof payload.submit_index === "number" || typeof payload.submit_text === "string"
                ),
                ui_diff: postDiff.summary,
                errors: errors
              });
            }, 65);
            return;
          }

          var f = fields[stepIdx++];
          var resolved = resolveTarget({
            index: f.index,
            text: f.target_text || f.text
          });
          if (resolved.error) {
            errors.push(resolved.error);
            nextField();
            return;
          }
          showPointerInteraction(resolved.x, resolved.y);
          dispatchTapSequence(resolved.x, resolved.y, resolved.item);
          setTimeout(function () {
            var inputEl =
              (resolved.item && resolved.item.isInput && resolved.item.node) || findActiveTextInput();
            if (inputEl) {
              setNativeInputValue(inputEl, String(f.value || ""), f.clear !== false);
              filledCount++;
            } else {
              errors.push("No input element found for field #" + stepIdx);
            }
            nextField();
          }, 55);
        }

        nextField();
      });
    },

    wait_for: function (reqId, payload) {
      var timeoutMs = typeof payload.timeout_ms === "number" ? payload.timeout_ms : 4000;
      var pollMs = typeof payload.poll_ms === "number" ? Math.max(25, payload.poll_ms) : 100;
      var wantGone = Boolean(payload.gone);
      var needle = typeof payload.text === "string" ? payload.text.toLowerCase() : "";
      var roleNeedle = typeof payload.role === "string" ? payload.role.toLowerCase() : "";
      var start = Date.now();

      ensureFlutterSemantics(function () {
        function check() {
          var items = buildUiTree(true);
          var matchedItem = null;
          for (var i = 0; i < items.length; i++) {
            var it = items[i];
            var textOk =
              !needle ||
              ((it.label || "") + " " + (it.value || "")).toLowerCase().indexOf(needle) !== -1;
            var roleOk = !roleNeedle || (it.role || "").toLowerCase() === roleNeedle;
            if (textOk && roleOk) {
              matchedItem = it;
              break;
            }
          }
          if (!matchedItem && needle && !roleNeedle && document.body) {
            var bodyTxt = (document.body.innerText || "").toLowerCase();
            if (bodyTxt.indexOf(needle) !== -1) {
              matchedItem = { index: -1, role: "text", label: payload.text };
            }
          }

          var satisfied = wantGone ? !matchedItem : Boolean(matchedItem);
          if (satisfied) {
            sendMcpReply(reqId, {
              ok: true,
              action: "wait_for",
              elapsed_ms: Date.now() - start,
              matched: matchedItem
                ? { index: matchedItem.index, role: matchedItem.role, label: matchedItem.label }
                : null
            });
            return;
          }
          if (Date.now() - start >= timeoutMs) {
            sendMcpReply(reqId, {
              ok: false,
              error:
                "Timed out after " +
                timeoutMs +
                "ms waiting for " +
                (wantGone ? "disappearance of " : "") +
                (payload.text || payload.role || "condition")
            });
            return;
          }
          setTimeout(check, pollMs);
        }
        check();
      });
    },

    network: function (reqId, payload) {
      var limit = typeof payload.limit === "number" && payload.limit > 0 ? payload.limit : 50;
      var slice = networkLog.slice(Math.max(0, networkLog.length - limit));
      if (payload.clear) {
        networkLog = [];
      }
      sendMcpReply(reqId, {
        ok: true,
        action: "network",
        count: slice.length,
        requests: slice
      });
    },

    tap: function (reqId, payload) {
      ensureFlutterSemantics(function () {
        computeUiDiff(buildUiTree(true));
        var resolved = resolveTarget(payload);
        if (resolved.error) {
          sendMcpReply(reqId, { ok: false, error: resolved.error });
          return;
        }
        showPointerInteraction(resolved.x, resolved.y);
        dispatchTapSequence(resolved.x, resolved.y, resolved.item);
        var targetDesc = resolved.item
          ? "[" + resolved.item.index + "] " + resolved.item.role + " " + resolved.item.label
          : "coords";
        setTimeout(function () {
          var postDiff = computeUiDiff(buildUiTree(true));
          sendMcpReply(reqId, {
            ok: true,
            action: "tap",
            pointer_mode: effectivePointerMode(),
            x: resolved.x,
            y: resolved.y,
            target: targetDesc,
            ui_diff: postDiff.summary
          });
        }, 60);
      });
    },

    type_text: function (reqId, payload) {
      ensureFlutterSemantics(function () {
        computeUiDiff(buildUiTree(true));
        var resolvedItem = null;
        var doType = function () {
          var inputEl =
            (resolvedItem && resolvedItem.isInput && resolvedItem.node) || findActiveTextInput();
          if (!inputEl) {
            sendMcpReply(reqId, {
              ok: false,
              error:
                "No active Flutter/DOM text input found. Pass 'index' or 'target_text' of the textfield, or tap it first."
            });
            return;
          }
          var clearFirst = payload.clear !== false;
          setNativeInputValue(inputEl, payload.text || "", clearFirst);
          if (payload.submit) {
            dispatchKey(inputEl, "Enter");
          }
          setTimeout(function () {
            var postDiff = computeUiDiff(buildUiTree(true));
            sendMcpReply(reqId, {
              ok: true,
              action: "type_text",
              pointer_mode: effectivePointerMode(),
              typed: payload.text || "",
              submitted: Boolean(payload.submit),
              ui_diff: postDiff.summary
            });
          }, 55);
        };

        if (typeof payload.index === "number" || typeof payload.target_text === "string") {
          var resolved = resolveTarget({
            index: payload.index,
            text: payload.target_text
          });
          if (resolved.error) {
            sendMcpReply(reqId, { ok: false, error: resolved.error });
            return;
          }
          resolvedItem = resolved.item;
          showPointerInteraction(resolved.x, resolved.y);
          dispatchTapSequence(resolved.x, resolved.y, resolved.item);
          setTimeout(doType, 90);
        } else {
          doType();
        }
      });
    },

    scroll: function (reqId, payload) {
      ensureFlutterSemantics(function () {
        var x = Math.round(window.innerWidth / 2);
        var y = Math.round(window.innerHeight / 2);
        if (typeof payload.index === "number") {
          var r = resolveTarget({ index: payload.index });
          if (!r.error) {
            x = r.x;
            y = r.y;
          }
        } else if (typeof payload.x === "number" && typeof payload.y === "number") {
          x = Math.round(payload.x);
          y = Math.round(payload.y);
        }

        var dx = typeof payload.dx === "number" ? payload.dx : 0;
        var dy = typeof payload.dy === "number" ? payload.dy : 300;
        if (typeof payload.direction === "string") {
          var dir = payload.direction.toLowerCase();
          var vStep = Math.round(window.innerHeight * 0.7);
          var hStep = Math.round(window.innerWidth * 0.7);
          var dirMap = {
            down: [0, vStep],
            up: [0, -vStep],
            bottom: [0, window.innerHeight * 4],
            top: [0, -window.innerHeight * 4],
            right: [hStep, 0],
            left: [-hStep, 0]
          };
          if (dirMap[dir]) {
            dx = dirMap[dir][0];
            dy = dirMap[dir][1];
          }
        }
        var endX = Math.max(10, Math.min(window.innerWidth - 10, x - Math.round(dx * 0.35)));
        var endY = Math.max(10, Math.min(window.innerHeight - 10, y - Math.round(dy * 0.35)));

        showPointerInteraction(x, y, endX, endY);
        var targetEl = deepElementFromPoint(x, y);
        targetEl.dispatchEvent(
          new WheelEvent("wheel", {
            deltaX: dx,
            deltaY: dy,
            clientX: x,
            clientY: y,
            bubbles: true,
            cancelable: true,
            composed: true
          })
        );
        sendMcpReply(reqId, {
          ok: true,
          action: "scroll",
          pointer_mode: effectivePointerMode(),
          x: x,
          y: y,
          dx: dx,
          dy: dy
        });
      });
    },

    press_key: function (reqId, payload) {
      var activeEl = findActiveTextInput() || document.activeElement || document.body;
      var key = payload.key || "Enter";
      dispatchKey(activeEl, key);
      sendMcpReply(reqId, {
        ok: true,
        action: "press_key",
        key: key
      });
    },

    inspect_overlay: function (reqId, payload) {
      if (typeof payload.pointer_mode === "string") {
        state.pointerMode = payload.pointer_mode;
      }
      state.inspectVisible =
        typeof payload.enabled === "boolean" ? payload.enabled : !state.inspectVisible;
      if (!state.inspectVisible) {
        state.selectorMap = [];
        renderInspectorBoxes();
        sendMcpReply(reqId, {
          ok: true,
          inspect_overlay: false,
          pointer_mode: effectivePointerMode(),
          elements_indexed: 0
        });
        return;
      }
      ensureFlutterSemantics(function (semActive) {
        var items = buildUiTree(true);
        renderInspectorBoxes();
        sendMcpReply(reqId, {
          ok: true,
          inspect_overlay: state.inspectVisible,
          pointer_mode: effectivePointerMode(),
          semantics_active: semActive,
          elements_indexed: items.length
        });
      });
    },

    eval_js: function (reqId, payload) {
      var fn = new Function(payload.script || "");
      var out = fn();
      Promise.resolve(out)
        .then(function (val) {
          sendMcpReply(reqId, {
            ok: true,
            result: val === undefined ? "undefined" : JSON.stringify(val)
          });
        })
        .catch(function (err) {
          sendMcpReply(reqId, { ok: false, error: String(err) });
        });
    }
  };

  window.__flutterff = {
    setPointerMode: function (mode) {
      state.pointerMode = mode || "auto";
      ensureMask();
      if (effectivePointerMode() === "touch" && state.cursorEl) {
        state.cursorEl.style.opacity = "0";
      }
      if (effectivePointerMode() === "cursor" && state.dotEl) {
        state.dotEl.style.opacity = "0";
      }
      if (state.inspectVisible) {
        setTimeout(function () {
          buildUiTree(true);
          renderInspectorBoxes();
        }, 120);
      }
    },

    dispatch: function (reqId, payload) {
      try {
        var handler = ACTION_HANDLERS[payload.action];
        if (!handler) {
          sendMcpReply(reqId, { ok: false, error: "Unknown agent action: " + payload.action });
          return;
        }
        handler(reqId, payload);
      } catch (err) {
        sendMcpReply(reqId, { ok: false, error: String(err) });
      }
    }
  };
})();
