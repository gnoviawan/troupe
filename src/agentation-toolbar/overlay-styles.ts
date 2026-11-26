export const OVERLAY_CSS = `
:host {
  all: initial;
  position: fixed !important;
  inset: 0 !important;
  z-index: 2147483647 !important;
  pointer-events: none !important;
  font-family: Inter, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif !important;
  --ov-bg: #1b1c1f;
  --ov-fg: #ededef;
  --ov-muted: #9ca3af;
  --ov-input-bg: #141518;
  --ov-border: #34363b;
  --ov-btn-sec: #2a2c31;
  --ov-btn-sec-border: #3f424a;
  --ov-shadow: 0 1px 2px rgba(0,0,0,0.5), 0 6px 8px -4px rgba(0,0,0,0.5);
}
@media (prefers-color-scheme: light) {
  :host {
    --ov-bg: #ffffff;
    --ov-fg: #18181b;
    --ov-muted: #52525b;
    --ov-input-bg: #f4f4f5;
    --ov-border: #d4d4d8;
    --ov-btn-sec: #f4f4f5;
    --ov-btn-sec-border: #d4d4d8;
    --ov-shadow: 0 1px 2px rgba(0,0,0,0.08), 0 6px 12px -4px rgba(0,0,0,0.12);
  }
}
* { box-sizing: border-box; }
.layer { position: fixed; inset: 0; pointer-events: none; }
.layer.capture { pointer-events: auto; cursor: crosshair; }
.layer.comment-mode { pointer-events: auto; cursor: default; }
.layer.ruler-v { pointer-events: auto; cursor: ew-resize; }
.layer.ruler-h { pointer-events: auto; cursor: ns-resize; }
.hover-box {
  position: fixed;
  border: 1.5px solid #3b82f6;
  background: rgba(59, 130, 246, 0.08);
  border-radius: 4px;
  pointer-events: none;
  display: none;
}
.selected-box {
  position: fixed;
  border: 2px solid #3b82f6;
  background: rgba(59, 130, 246, 0.12);
  box-shadow: 0 0 0 1px rgba(59, 130, 246, 0.28);
  border-radius: 4px;
  pointer-events: none;
  display: none;
}
.selected-box.rect {
  border-style: dashed;
  background: rgba(59, 130, 246, 0.10);
  border-radius: 6px;
}
.drag-box {
  position: fixed;
  border: 1.5px dashed #3b82f6;
  background: rgba(59, 130, 246, 0.10);
  border-radius: 6px;
  pointer-events: none;
  display: none;
}
.pin {
  position: fixed;
  width: 22px;
  height: 22px;
  margin-left: -11px;
  margin-top: -11px;
  border-radius: 9999px;
  background: #3b82f6;
  color: #ffffff;
  font-size: 12px;
  font-weight: 500;
  display: flex;
  align-items: center;
  justify-content: center;
  box-shadow: 0 0 0 2px #ffffff, 0 2px 4px rgba(0,0,0,0.35);
  pointer-events: auto;
  cursor: pointer;
}
.pin.guide-pin { background: #ef4444; }
.saved-rect {
  position: fixed;
  border: 1.5px dashed #3b82f6;
  background: rgba(59, 130, 246, 0.06);
  border-radius: 6px;
  pointer-events: none;
}
.guide-preview {
  position: fixed;
  pointer-events: none;
  display: none;
}
.guide-preview.vertical {
  top: 0;
  bottom: 0;
  width: 0;
  border-left: 1.5px dashed rgba(239, 68, 68, 0.85);
}
.guide-preview.horizontal {
  left: 0;
  right: 0;
  height: 0;
  border-top: 1.5px dashed rgba(239, 68, 68, 0.85);
}
.guide-preview-badge {
  position: fixed;
  height: 22px;
  padding: 0 8px;
  border-radius: 4px;
  background: rgba(24, 24, 27, 0.92);
  color: #ffffff;
  box-shadow: 0 0 0 1px rgba(239, 68, 68, 0.65), 0 2px 6px rgba(0,0,0,0.3);
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 11px;
  line-height: 22px;
  pointer-events: none;
  display: none;
  white-space: nowrap;
}
.guide-hit {
  position: fixed;
  pointer-events: auto;
  touch-action: none;
  display: flex;
  align-items: center;
  justify-content: center;
}
.guide-hit.vertical {
  top: 0;
  bottom: 0;
  width: 16px;
  margin-left: -8px;
  cursor: ew-resize;
}
.guide-hit.horizontal {
  left: 0;
  right: 0;
  height: 16px;
  margin-top: -8px;
  cursor: ns-resize;
}
.guide-line {
  background: #ef4444;
  pointer-events: none;
  transition: background-color 120ms ease, box-shadow 120ms ease, width 120ms ease, height 120ms ease;
}
.guide-hit.vertical .guide-line {
  width: 1.5px;
  height: 100%;
}
.guide-hit.horizontal .guide-line {
  height: 1.5px;
  width: 100%;
}
.guide-hit:hover .guide-line,
.guide-hit.dragging .guide-line {
  background: #dc2626;
  box-shadow: 0 0 0 2px rgba(239, 68, 68, 0.25);
}
.guide-hit.vertical:hover .guide-line,
.guide-hit.vertical.dragging .guide-line {
  width: 2.5px;
}
.guide-hit.horizontal:hover .guide-line,
.guide-hit.horizontal.dragging .guide-line {
  height: 2.5px;
}
.guide-handle {
  position: absolute;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 9999px;
  background: #ef4444;
  color: #ffffff;
  box-shadow: 0 0 0 1.5px rgba(255,255,255,0.9), 0 1px 4px rgba(0,0,0,0.35);
  pointer-events: none;
  transition: transform 120ms ease, background-color 120ms ease;
}
.guide-hit.vertical .guide-handle {
  top: 12px;
  width: 14px;
  height: 28px;
  flex-direction: column;
  gap: 2px;
}
.guide-hit.horizontal .guide-handle {
  left: 12px;
  width: 28px;
  height: 14px;
  flex-direction: row;
  gap: 2px;
}
.guide-hit:hover .guide-handle,
.guide-hit.dragging .guide-handle {
  background: #dc2626;
  transform: scale(1.08);
}
.guide-handle-dot {
  width: 3px;
  height: 3px;
  border-radius: 9999px;
  background: rgba(255, 255, 255, 0.92);
}
.guide-badge {
  position: fixed;
  height: 20px;
  padding: 0 7px;
  border-radius: 4px;
  background: #ef4444;
  color: #ffffff;
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 11px;
  line-height: 20px;
  box-shadow: 0 1px 3px rgba(0,0,0,0.3);
  pointer-events: auto;
  touch-action: none;
  user-select: none;
  white-space: nowrap;
}
.guide-badge.vertical { cursor: ew-resize; }
.guide-badge.horizontal { cursor: ns-resize; }
.guide-badge:hover,
.guide-badge.dragging {
  background: #dc2626;
  box-shadow: 0 0 0 1.5px rgba(255,255,255,0.85), 0 2px 6px rgba(0,0,0,0.35);
}
.popover {
  position: fixed;
  width: min(332px, calc(100vw - 24px));
  padding: 14px;
  border-radius: 8px;
  background: var(--ov-bg);
  color: var(--ov-fg);
  box-shadow: 0 0 0 0.5px var(--ov-border), var(--ov-shadow);
  display: flex;
  flex-direction: column;
  gap: 10px;
  pointer-events: auto;
}
.pop-head { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
.pop-title {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 12px;
  color: var(--ov-fg);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.pop-pill {
  font-size: 12px;
  line-height: 16px;
  padding: 2px 7px;
  border-radius: 9999px;
  background: rgba(34, 197, 94, 0.14);
  color: #22c55e;
  box-shadow: 0 0 0 0.5px rgba(34, 197, 94, 0.34);
  flex-shrink: 0;
}
.pop-input {
  width: 100%;
  min-height: 62px;
  padding: 8px 10px;
  border-radius: 6px;
  border: 0;
  background: var(--ov-input-bg);
  color: var(--ov-fg);
  font-size: 13px;
  line-height: 20px;
  font-family: inherit;
  resize: none;
  outline: none;
  box-shadow: 0 0 0 2px #6b7280;
}
.pop-foot { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 8px; }
.pop-links { display: flex; align-items: center; gap: 10px; }
.pop-error { font-size: 12px; color: #f87171; }
.pop-hint { font-size: 12px; color: var(--ov-muted); font-family: ui-monospace, SFMono-Regular, Menlo, monospace; }
.pop-link {
  background: none; border: 0; padding: 0; color: #f87171;
  font-size: 12px; text-decoration: underline; cursor: pointer;
}
.pop-link.measure { color: #ec4899; }
.pop-actions { display: flex; gap: 8px; }
.btn {
  height: 28px; padding: 0 11px; border-radius: 6px; border: 0;
  font-size: 12px; font-weight: 500; cursor: pointer;
}
.btn-sec { background: var(--ov-btn-sec); color: var(--ov-fg); box-shadow: 0 0 0 0.5px var(--ov-btn-sec-border); }
.btn-pri { background: #0e72cc; color: #ffffff; box-shadow: 0 0 0 0.5px #0e72cc; }
`
