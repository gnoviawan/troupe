export const OVERLAY_CSS = `
:host {
  all: initial;
  position: fixed !important;
  inset: 0 !important;
  z-index: 2147483647 !important;
  pointer-events: none !important;
  font-family: Inter, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif !important;
}
* { box-sizing: border-box; }
.layer { position: fixed; inset: 0; pointer-events: none; }
.layer.capture { pointer-events: auto; cursor: crosshair; }
.layer.comment-mode { pointer-events: auto; cursor: default; }
.hover-box {
  position: fixed;
  border: 1.5px solid #3b82f6;
  background: rgba(59, 130, 246, 0.08);
  border-radius: 4px;
  pointer-events: none;
  display: none;
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
.guide {
  position: fixed;
  background: #ef4444;
  pointer-events: auto;
  cursor: pointer;
}
.guide.vertical { top: 0; bottom: 0; width: 1.5px; cursor: ew-resize; }
.guide.horizontal { left: 0; right: 0; height: 1.5px; cursor: ns-resize; }
.guide-badge {
  position: fixed;
  height: 20px;
  padding: 0 6px;
  border-radius: 4px;
  background: #ef4444;
  color: #ffffff;
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 12px;
  line-height: 20px;
  pointer-events: auto;
  cursor: pointer;
  white-space: nowrap;
}
.spacing-pill {
  position: fixed;
  height: 20px;
  padding: 0 6px;
  border-radius: 4px;
  background: #ec4899;
  color: #ffffff;
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 12px;
  line-height: 20px;
  pointer-events: none;
  display: none;
}
.spacing-line {
  position: fixed;
  pointer-events: none;
  display: none;
}
.spacing-line.v { border-left: 1px dashed #ec4899; width: 0; }
.spacing-line.h { border-top: 1px dashed #ec4899; height: 0; }
.popover {
  position: fixed;
  width: min(332px, calc(100vw - 24px));
  padding: 14px;
  border-radius: 8px;
  background: #1b1c1f;
  color: #ededef;
  box-shadow: 0 0 0 0.5px #34363b, 0 1px 2px rgba(0,0,0,0.5), 0 6px 8px -4px rgba(0,0,0,0.5);
  display: flex;
  flex-direction: column;
  gap: 10px;
  pointer-events: auto;
}
.pop-head { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
.pop-title {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 12px;
  color: #ededef;
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
  color: #4ade80;
  box-shadow: 0 0 0 0.5px rgba(34, 197, 94, 0.34);
  flex-shrink: 0;
}
.pop-input {
  width: 100%;
  min-height: 62px;
  padding: 8px 10px;
  border-radius: 6px;
  border: 0;
  background: #141518;
  color: #ededef;
  font-size: 13px;
  line-height: 20px;
  font-family: inherit;
  resize: none;
  outline: none;
  box-shadow: 0 0 0 2px #6b7280;
}
.pop-foot { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 8px; }
.pop-error { font-size: 12px; color: #f87171; }
.pop-hint { font-size: 12px; color: #9ca3af; font-family: ui-monospace, SFMono-Regular, Menlo, monospace; }
.pop-link {
  background: none; border: 0; padding: 0; color: #f87171;
  font-size: 12px; text-decoration: underline; cursor: pointer;
}
.pop-actions { display: flex; gap: 8px; }
.btn {
  height: 28px; padding: 0 11px; border-radius: 6px; border: 0;
  font-size: 12px; font-weight: 500; cursor: pointer;
}
.btn-sec { background: #2a2c31; color: #ededef; box-shadow: 0 0 0 0.5px #3f424a, 0 1px 2px rgba(0,0,0,0.5); }
.btn-pri { background: #0e72cc; color: #ffffff; box-shadow: 0 0 0 0.5px #0e72cc, 0 1px 2px rgba(0,0,0,0.5); }
`
