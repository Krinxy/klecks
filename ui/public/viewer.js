// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-17
// ---
// 3D scene viewer used by the Rust/Leptos UI via `window.klecksViewer`.
// Shows either a coloured point cloud (scene.ply from room_scan) or a trained Gaussian splat
// (splat.ply from Brush); the file type is detected from the header.
// Three.js and Spark (splat renderer) are vendored (ui/public/vendor) because the app runs offline.
//
// Controls: drag = look around, right/middle drag = pan, wheel = zoom,
//           W A S D = walk, Q / E = down / up, Shift = faster, double click = reset.
import * as THREE from "./vendor/three.module.min.js";
import { SparkRenderer, SplatEdit, SplatEditRgbaBlendMode, SplatEditSdf, SplatEditSdfType, SplatMesh } from "./vendor/spark.module.min.js";

const RECORD_BYTES = 15; // float32 x,y,z + uint8 r,g,b (see lab/vggt_try.py write_ply)
const MAX_PITCH = 1.5;
const STALL_MS = 20000; // no data for this long: stop waiting and say so
const PREPARE_MS = 120000; // preparing the splats can take a while on a busy graphics card, but not forever

class KlecksViewer {
  constructor() {
    this.host = null;
    this.keys = new Set();
    this.listeners = [];
  }

  /** Creates the renderer inside the element with this id. */
  mount(id) {
    this.dispose();
    this.mountError = "";
    const host = document.getElementById(id);
    if (!host) {
      this.mountError = "The area for the 3D view is missing.";
      return false;
    }
    try {
      return this.#create(host);
    } catch (error) {
      // creating the WebGL view can fail on a busy or restricted graphics driver: say why instead of hanging
      this.mountError = String(error?.message || error);
      try {
        this.dispose();
      } catch (_) {
        /* nothing left to clean */
      }
      return false;
    }
  }

  /** Why the last mount failed (empty when it did not). */
  lastError() {
    return this.mountError || "";
  }

  #create(host) {
    this.host = host;

    this.renderer = new THREE.WebGLRenderer({ antialias: false, alpha: true, powerPreference: "high-performance" });
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
    this.renderer.setClearColor(0x000000, 0);
    host.appendChild(this.renderer.domElement);

    this.scene = new THREE.Scene();
    this.spark = new SparkRenderer({ renderer: this.renderer });
    this.scene.add(this.spark);
    this.camera = new THREE.PerspectiveCamera(62, 1, 0.01, 500);
    this.pivot = new THREE.Vector3(0, 0, -1);
    this.radius = 1;
    this.yaw = 0;
    this.pitch = 0;
    this.scale = 1;
    this.dirty = true;
    this.last = performance.now();

    this.#bind();
    this.#resize();
    this.#apply();
    this.frame = requestAnimationFrame((now) => this.#tick(now));
    return true;
  }

  /**
   * Downloads a .ply written by the reconstruction and shows it.
   * `sceneScale` is the typical distance of surfaces from the filming position (from the scan report);
   * the viewer uses it to size the first view and the walking speed. Resolves to { kind, count }.
   */
  async load(url, sceneScale) {
    if (!this.renderer) throw new Error("viewer is not mounted");
    const id = (this.loadId = (this.loadId || 0) + 1);
    const timings = (this.timings = { startedAt: performance.now() });
    this.#startTicker();
    try {
      return await this.#loadScene(id, url, sceneScale, timings);
    } finally {
      if (id === this.loadId) this.#stopTicker();
    }
  }

  async #loadScene(id, url, sceneScale, timings) {
    const bytes = await this.#download(url);
    if (id !== this.loadId) throw new Error("cancelled");
    timings.downloadMs = Math.round(performance.now() - timings.startedAt);
    timings.megabytes = Math.round(bytes.byteLength / 1048576);
    this.#clear();
    const header = new TextDecoder("ascii").decode(bytes.subarray(0, Math.min(bytes.byteLength, 2048)));
    const result = header.includes("f_dc_0") ? await this.#showSplats(bytes, sceneScale, id) : this.#showPoints(bytes.buffer);
    this.#applyLayout();
    timings.totalMs = Math.round(performance.now() - timings.startedAt);
    console.log("[klecks viewer] load", JSON.stringify(timings));
    return result;
  }

  /** Reads the file in pieces so the screen can say how far it is (large scenes are 100-400 MB). */
  async #download(url) {
    const stalled = new Error("The scene stopped loading: no data came in for 20 seconds. Close the app, open it again and try once more.");
    const controller = new AbortController();
    const controllerId = this.loadId;
    this.abort = controller;
    let timer = 0;
    const watch = () => {
      clearTimeout(timer);
      timer = setTimeout(() => controller.abort(), STALL_MS);
    };
    try {
      watch();
      return await this.#readAll(url, controller.signal, watch);
    } catch (error) {
      if (error?.name === "AbortError") throw this.loadId === controllerId ? stalled : new Error("cancelled");
      throw error;
    } finally {
      clearTimeout(timer);
    }
  }

  async #readAll(url, signal, watch) {
    const response = await fetch(url, { signal });
    if (!response.ok) throw new Error(`Could not read the scene (HTTP ${response.status}).`);
    const total = Number(response.headers.get("content-length")) || 0;
    const reader = response.body.getReader();
    const chunks = [];
    let received = 0;
    let shownAt = 0;
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      watch();
      chunks.push(value);
      received += value.length;
      const now = performance.now();
      if (now - shownAt > 120) {
        shownAt = now;
        this.#status(total ? `Reading the scene, ${Math.round(received / 1048576)} of ${Math.round(total / 1048576)} MB` : `Reading the scene, ${Math.round(received / 1048576)} MB`);
      }
    }
    const bytes = new Uint8Array(received);
    let offset = 0;
    for (const chunk of chunks) {
      bytes.set(chunk, offset);
      offset += chunk.length;
    }
    return bytes;
  }

  /** Text shown over the viewer while it works (the element is the loading overlay of the UI). */
  #status(text) {
    this.phase = text;
    this.#showStatus();
  }

  #showStatus() {
    const element = document.getElementById("klecks-viewer-status");
    if (!element) return;
    const seconds = Math.round((performance.now() - (this.timings?.startedAt ?? performance.now())) / 1000);
    element.textContent = seconds >= 3 ? `${this.phase || "Loading the scene…"} (${seconds} s)` : this.phase || "Loading the scene…";
  }

  #startTicker() {
    this.#stopTicker();
    this.phase = "Loading the scene…";
    this.ticker = setInterval(() => this.#showStatus(), 500);
  }

  #stopTicker() {
    clearInterval(this.ticker);
    this.ticker = 0;
  }

  #clear() {
    this.#dropLayout();
    if (this.cloud) {
      this.scene.remove(this.cloud);
      this.cloud.geometry.dispose();
      this.cloud.material.dispose();
      this.cloud = null;
    }
    if (this.splats) {
      this.scene.remove(this.splats);
      this.splats.dispose?.();
      this.splats = null;
    }
  }

  async #showSplats(bytes, sceneScale, id) {
    this.#status("Preparing the splats…");
    const prepared = performance.now();
    const splats = new SplatMesh({ fileBytes: bytes, fileName: "scene.ply" });
    await Promise.race([
      splats.initialized,
      new Promise((_, reject) => setTimeout(() => reject(new Error("The splats could not be prepared in two minutes. The graphics card may be busy: close other programs that use it and try again.")), PREPARE_MS)),
    ]);
    if (id !== this.loadId) {
      splats.dispose?.();
      throw new Error("cancelled");
    }
    this.timings.prepareMs = Math.round(performance.now() - prepared);
    splats.rotation.x = Math.PI; // the model's y axis points down; flip so up is up
    this.scene.add(splats);
    this.splats = splats;
    this.scale = Math.max(Number(sceneScale) || 0.7, 0.05);
    this.resetView();
    return { kind: "splats", count: splats.numSplats ?? 0 };
  }

  #showPoints(buffer) {
    const { positions, colors, count } = this.#parse(buffer);
    // Typical distance of the points from the filming position sets the size of everything else.
    let sum = 0;
    for (let i = 0; i < count; i += 7) sum += Math.hypot(positions[i * 3], positions[i * 3 + 1], positions[i * 3 + 2]);
    this.scale = Math.max(sum / Math.ceil(count / 7), 0.05);

    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute("position", new THREE.BufferAttribute(positions, 3));
    geometry.setAttribute("color", new THREE.BufferAttribute(colors, 3, true));
    const material = new THREE.PointsMaterial({ size: this.scale * 0.0065, vertexColors: true, sizeAttenuation: true });
    this.cloud = new THREE.Points(geometry, material);
    this.cloud.frustumCulled = false;
    this.cloud.rotation.x = Math.PI; // the model's y axis points down; flip so up is up
    this.scene.add(this.cloud);

    this.resetView();
    return { kind: "points", count };
  }

  /**
   * Shows what the person did to the room: furniture as boxes, cut-outs as hidden parts of the scan.
   * Everything is in the coordinates of the scan. `frame` gives the floor's axes ({ a, b, up }, unit vectors),
   * an item is { pos, size, turn, selected } with `size` along its width, height and depth, a cut is { pos, size, turn }.
   */
  setLayout(layout) {
    this.layout = layout;
    this.#applyLayout();
  }

  #dropLayout() {
    if (this.proxies) {
      this.proxies.traverse((object) => {
        object.geometry?.dispose?.();
        object.material?.dispose?.();
      });
      this.scene.remove(this.proxies);
      this.proxies = null;
    }
    if (this.cutEdit) {
      this.cutEdit.removeFromParent();
      this.cutEdit = null;
    }
  }

  /** Orientation of a box that stands on the floor, turned by `turn` degrees like a rectangle on the plan. */
  #boxTurn(frame, turn) {
    const [a, b, up] = [frame.a, frame.b, frame.up].map((v) => new THREE.Vector3(...v));
    const angle = (turn * Math.PI) / 180;
    const width = a.clone().multiplyScalar(Math.cos(angle)).addScaledVector(b, Math.sin(angle));
    const depth = a.clone().multiplyScalar(-Math.sin(angle)).addScaledVector(b, Math.cos(angle));
    // width x up = -depth, so the right-handed set is (width, up, -depth)
    const matrix = new THREE.Matrix4().makeBasis(width, up, depth.clone().negate());
    return new THREE.Quaternion().setFromRotationMatrix(matrix);
  }

  #applyLayout() {
    if (!this.scene) return;
    this.#dropLayout();
    const layout = this.layout;
    if (!layout || !layout.frame) {
      this.dirty = true;
      return;
    }
    const group = new THREE.Group();
    group.rotation.x = Math.PI; // same flip as the scene: its y axis points down
    for (const item of layout.items || []) {
      const box = new THREE.Mesh(
        new THREE.BoxGeometry(1, 1, 1),
        new THREE.MeshBasicMaterial({ color: item.selected ? 0xe6f19a : 0x9fb350, transparent: true, opacity: item.selected ? 0.5 : 0.32, depthWrite: false }),
      );
      const edges = new THREE.LineSegments(
        new THREE.EdgesGeometry(box.geometry),
        new THREE.LineBasicMaterial({ color: item.selected ? 0xffffff : 0xd7e68a }),
      );
      box.add(edges);
      box.position.set(...item.pos);
      box.scale.set(...item.size);
      box.quaternion.copy(this.#boxTurn(layout.frame, item.turn || 0));
      group.add(box);
    }
    this.scene.add(group);
    this.proxies = group;

    if (this.splats && (layout.cuts || []).length) {
      const edit = new SplatEdit({ rgbaBlendMode: SplatEditRgbaBlendMode.MULTIPLY, sdfSmooth: 0, softEdge: 0 });
      for (const cut of layout.cuts) {
        const sdf = new SplatEditSdf({ type: SplatEditSdfType.BOX, opacity: 0 });
        sdf.position.set(...cut.pos);
        sdf.scale.set(cut.size[0] / 2, cut.size[1] / 2, cut.size[2] / 2);
        sdf.quaternion.copy(this.#boxTurn(layout.frame, cut.turn || 0));
        edit.add(sdf);
      }
      this.splats.add(edit);
      this.cutEdit = edit;
    }
    this.dirty = true;
  }

  /** Moves the camera: yaw/pitch in radians, radius and pivot ([x, y, z]) in scene units. */
  setView({ yaw, pitch, radius, pivot } = {}) {
    if (!this.renderer) return;
    if (yaw !== undefined) this.yaw = yaw;
    if (pitch !== undefined) this.pitch = Math.max(-MAX_PITCH, Math.min(MAX_PITCH, pitch));
    if (radius !== undefined) this.radius = radius;
    if (pivot) this.pivot.set(pivot[0], pivot[1], pivot[2]);
    this.#apply();
  }

  /** Back to the position the video was filmed from. */
  resetView() {
    this.yaw = 0;
    this.pitch = 0;
    this.radius = this.scale * 0.6;
    this.pivot.set(0, 0, -this.radius);
    this.#apply();
  }

  dispose() {
    this.loadId = (this.loadId || 0) + 1; // a load still running is no longer wanted
    this.abort?.abort();
    this.#stopTicker();
    if (this.frame) cancelAnimationFrame(this.frame);
    this.frame = 0;
    for (const [target, name, handler, options] of this.listeners) target.removeEventListener(name, handler, options);
    this.listeners = [];
    this.keys.clear();
    this.resizeObserver?.disconnect();
    if (this.scene) this.#clear();
    if (this.renderer) {
      this.renderer.forceContextLoss?.(); // hand the WebGL context back; a webview only keeps a few
      this.renderer.dispose();
      this.renderer.domElement.remove();
      this.renderer = null;
    }
    this.host = null;
  }

  #parse(buffer) {
    const head = new TextDecoder("ascii").decode(new Uint8Array(buffer, 0, Math.min(buffer.byteLength, 512)));
    const end = head.indexOf("end_header\n");
    const count = Number((head.match(/element vertex (\d+)/) || [])[1]);
    if (end < 0 || !count) throw new Error("The scene file is not a point cloud.");
    const expected = "property float x\nproperty float y\nproperty float z\nproperty uchar red\nproperty uchar green\nproperty uchar blue\n";
    if (!head.includes(expected)) throw new Error("The scene file has an unexpected layout.");

    const view = new DataView(buffer, end + "end_header\n".length);
    if (view.byteLength < count * RECORD_BYTES) throw new Error("The scene file is cut off.");
    const positions = new Float32Array(count * 3);
    const colors = new Uint8Array(count * 3);
    for (let i = 0; i < count; i++) {
      const base = i * RECORD_BYTES;
      positions[i * 3] = view.getFloat32(base, true);
      positions[i * 3 + 1] = view.getFloat32(base + 4, true);
      positions[i * 3 + 2] = view.getFloat32(base + 8, true);
      colors[i * 3] = view.getUint8(base + 12);
      colors[i * 3 + 1] = view.getUint8(base + 13);
      colors[i * 3 + 2] = view.getUint8(base + 14);
    }
    return { positions, colors, count };
  }

  #on(target, name, handler, options) {
    target.addEventListener(name, handler, options);
    this.listeners.push([target, name, handler, options]);
  }

  #bind() {
    const canvas = this.renderer.domElement;
    let drag = null;
    this.#on(canvas, "contextmenu", (event) => event.preventDefault());
    this.#on(canvas, "pointerdown", (event) => {
      drag = { x: event.clientX, y: event.clientY, pan: event.button !== 0 };
      canvas.setPointerCapture(event.pointerId);
    });
    this.#on(canvas, "pointermove", (event) => {
      if (!drag) return;
      const dx = event.clientX - drag.x;
      const dy = event.clientY - drag.y;
      drag.x = event.clientX;
      drag.y = event.clientY;
      if (drag.pan) {
        const step = this.radius * 0.0016;
        const right = new THREE.Vector3().setFromMatrixColumn(this.camera.matrix, 0);
        const up = new THREE.Vector3().setFromMatrixColumn(this.camera.matrix, 1);
        this.pivot.addScaledVector(right, -dx * step).addScaledVector(up, dy * step);
      } else {
        this.yaw -= dx * 0.005;
        this.pitch = Math.max(-MAX_PITCH, Math.min(MAX_PITCH, this.pitch + dy * 0.005));
      }
      this.#apply();
    });
    const release = () => (drag = null);
    this.#on(canvas, "pointerup", release);
    this.#on(canvas, "pointercancel", release);
    this.#on(canvas, "wheel", (event) => {
      event.preventDefault();
      this.radius = Math.max(this.scale * 0.02, Math.min(this.scale * 30, this.radius * Math.exp(event.deltaY * 0.0012)));
      this.#apply();
    }, { passive: false });
    this.#on(canvas, "dblclick", () => this.resetView());
    this.#on(window, "keydown", (event) => this.keys.add(event.key.toLowerCase()));
    this.#on(window, "keyup", (event) => this.keys.delete(event.key.toLowerCase()));
    this.#on(window, "blur", () => this.keys.clear());

    this.resizeObserver = new ResizeObserver(() => this.#resize());
    this.resizeObserver.observe(this.host);
  }

  #resize() {
    if (!this.host || !this.renderer) return;
    const width = Math.max(this.host.clientWidth, 1);
    const height = Math.max(this.host.clientHeight, 1);
    this.renderer.setSize(width, height);
    this.camera.aspect = width / height;
    this.camera.updateProjectionMatrix();
    this.dirty = true;
  }

  /** Places the camera on a sphere around the pivot. */
  #apply() {
    const cosPitch = Math.cos(this.pitch);
    const offset = new THREE.Vector3(Math.sin(this.yaw) * cosPitch, Math.sin(this.pitch), Math.cos(this.yaw) * cosPitch);
    this.camera.position.copy(this.pivot).addScaledVector(offset, this.radius);
    this.camera.up.set(0, 1, 0);
    this.camera.lookAt(this.pivot);
    this.camera.updateMatrixWorld();
    this.dirty = true;
  }

  #walk(seconds) {
    const keys = this.keys;
    const forward = (keys.has("w") ? 1 : 0) - (keys.has("s") ? 1 : 0);
    const strafe = (keys.has("d") ? 1 : 0) - (keys.has("a") ? 1 : 0);
    const lift = (keys.has("e") ? 1 : 0) - (keys.has("q") ? 1 : 0);
    if (!forward && !strafe && !lift) return;
    const speed = this.scale * 0.6 * (keys.has("shift") ? 3 : 1) * seconds;
    const ahead = new THREE.Vector3().subVectors(this.pivot, this.camera.position).normalize();
    const right = new THREE.Vector3().setFromMatrixColumn(this.camera.matrix, 0);
    this.pivot
      .addScaledVector(ahead, forward * speed)
      .addScaledVector(right, strafe * speed)
      .addScaledVector(new THREE.Vector3(0, 1, 0), lift * speed);
    this.#apply();
  }

  #tick(now) {
    if (!this.renderer) return;
    this.#walk(Math.min((now - this.last) / 1000, 0.1));
    this.last = now;
    // Splats are sorted asynchronously, so they keep rendering; a point cloud only when something changed.
    if (this.dirty || this.splats) {
      this.renderer.render(this.scene, this.camera);
      this.dirty = false;
    }
    this.frame = requestAnimationFrame((time) => this.#tick(time));
  }
}

window.klecksViewer = new KlecksViewer();
