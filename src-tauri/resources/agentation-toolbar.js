"use strict";var AgentationToolbar=(()=>{var C=Object.defineProperty;var U=Object.getOwnPropertyDescriptor;var X=Object.getOwnPropertyNames;var z=Object.prototype.hasOwnProperty;var W=(n,e)=>{for(var t in e)C(n,t,{get:e[t],enumerable:!0})},V=(n,e,t,i)=>{if(e&&typeof e=="object"||typeof e=="function")for(let o of X(e))!z.call(n,o)&&o!==t&&C(n,o,{get:()=>e[o],enumerable:!(i=U(e,o))||i.enumerable});return n};var Y=n=>V(C({},"__esModule",{value:!0}),n);var Z={};W(Z,{GUIDE_POPOVER_REOPEN_DELAY_MS:()=>D,TroupeAnnotationOverlay:()=>N,buildElementPath:()=>_,buildShortLabel:()=>E,mountOverlay:()=>G});function B(n,e){let t=document.createElement("div");t.className="popover";let i=Math.min(332,Math.max(240,window.innerWidth-24)),o=Math.max(12,Math.min(window.innerWidth-i-12,n.x+14)),s=Math.max(12,Math.min(window.innerHeight-190,n.y+14));t.style.left=`${o}px`,t.style.top=`${s}px`;let r=document.createElement("div");r.className="pop-head";let a=document.createElement("span");a.className="pop-title",a.textContent=n.title;let u=document.createElement("span");u.className="pop-pill",u.textContent="Auto-crop on add",r.append(a,u);let p=document.createElement("textarea");p.className="pop-input",n.guide?.draftComment&&(p.value=n.guide.draftComment),p.placeholder=n.kind==="guide"?`Describe alignment change at ${n.title}\u2026`:"What should change on this element\u2026";let d=document.createElement("div");if(d.className="pop-foot",n.guide){let m=n.guide,f=document.createElement("div");f.className="pop-links";let v=document.createElement("button");if(v.type="button",v.className="pop-link",v.textContent="Remove guide",v.addEventListener("click",()=>e.onRemoveGuide(m.id)),f.appendChild(v),typeof n.guideGapPx=="number"&&n.guideGapPx>0){let y=n.guideGapPx,w=document.createElement("button");w.type="button",w.className="pop-link measure",w.textContent=`Save spacing (${y}px)`,w.addEventListener("click",()=>{e.onSwitchToSpacing({title:`Spacing \xB7 ${y}px (${m.axis})`,x:n.x,y:n.y,kind:"measurement",measurement:{axis:m.axis,positionPx:m.pos,gapPx:y}})}),f.appendChild(w)}d.appendChild(f)}else{let m=document.createElement("span");m.className="pop-hint";let f=/mac/i.test(navigator.platform||navigator.userAgent);m.textContent=f?"\u2318Enter to add":"Ctrl+Enter to add",d.appendChild(m)}let l=document.createElement("div");l.className="pop-error",l.style.display="none";let x=document.createElement("div");x.className="pop-actions";let h=document.createElement("button");h.type="button",h.className="btn btn-sec",h.textContent="Cancel",h.addEventListener("click",()=>e.onCancel());let c=document.createElement("button");c.type="button",c.className="btn btn-pri",c.textContent="Add comment";let g=!1,b=async()=>{if(g)return;let m=p.value.trim();if(!m)return;g=!0,c.disabled=!0,c.textContent="Saving\u2026",l.style.display="none";let f=Math.round(n.x),v=Math.round(n.y),y={x:f,y:v,comment:m,element:n.summary?.element??n.title,elementPath:n.summary?.elementPath??n.guide?.targetSelector??n.title,fullPath:n.summary?.fullPath,cssClasses:n.summary?.cssClasses,nearbyText:n.summary?.nearbyText,computedStyles:n.summary?.computedStyles,reactComponents:n.summary?.reactComponents,boundingBox:n.rect??n.summary?.boundingBox,viewportWidth:n.viewportWidth??window.innerWidth,kind:n.kind};n.guide?y.measurement={axis:n.guide.axis,positionPx:n.guide.pos,toSelector:n.guide.targetSelector,deltaPx:n.guide.deltaPx}:n.measurement&&(y.measurement={axis:n.measurement.axis,positionPx:n.measurement.positionPx,toSelector:n.summary?.elementPath,gapPx:n.measurement.gapPx});let w=e.onHideSelectionForCapture();t.style.display="none";let S=await e.onSubmit(y,{comment:m,pinX:f,pinY:v});if(g=!1,!S.ok){t.style.display="flex",e.onRestoreSelectionAfterError(w),c.disabled=!1,c.textContent="Retry",l.textContent="Could not save annotation. Try again.",l.style.display="block";return}e.onSaved(t,S.id,m,f,v)};return c.addEventListener("click",()=>{b()}),p.addEventListener("keydown",m=>{(m.metaKey||m.ctrlKey)&&m.key==="Enter"&&(m.preventDefault(),b())}),x.append(h,c),d.appendChild(x),t.append(r,p,l,d),{pop:t,textarea:p}}var K=["display","position","width","height","padding","margin","gap","font-size","line-height","font-weight","border-radius"];function E(n){let e=n.tagName.toLowerCase();if(n.id)return`${e}#${n.id}`;let t=typeof n.className=="string"?n.className.trim().split(/\s+/).filter(Boolean):[];return t.length>0?`${e}.${t.slice(0,2).join(".")}`:e}function _(n,e=4){let t=[],i=n;for(;i&&i!==document.body&&t.length<e&&(t.unshift(E(i)),!i.id);)i=i.parentElement;return t.join(" > ")||n.tagName.toLowerCase()}function j(n){let e=Object.keys(n).find(s=>s.startsWith("__reactFiber$")||s.startsWith("__reactInternalInstance$"));if(!e)return;let t=[],i=n[e],o=0;for(;i&&t.length<5&&o<40;){o+=1;let s=i.type,r=typeof s=="function"||typeof s=="object"&&s!==null?s.displayName||s.name:void 0;r&&/^[A-Z]/.test(r)&&!t.includes(r)&&t.push(r),i=i.return}return t.length>0?t.join(" < "):void 0}function q(n){let e=window.getComputedStyle(n),t=[];for(let i of K){let o=e.getPropertyValue(i);o&&o!=="normal"&&o!=="none"&&o!=="0px"&&o!=="static"&&t.push(`${i}: ${o}`)}return t.join("; ")}function R(n){let e=n.getBoundingClientRect(),t=(n.textContent??"").trim().replace(/\s+/g," ").slice(0,120),i=typeof n.className=="string"&&n.className.trim()?n.className.trim().slice(0,200):void 0;return{element:E(n),elementPath:_(n,4),fullPath:_(n,10),cssClasses:i,nearbyText:t||void 0,computedStyles:q(n)||void 0,reactComponents:j(n),boundingBox:{x:Math.round(e.x),y:Math.round(e.y),width:Math.round(e.width),height:Math.round(e.height)}}}function k(n,e,t){let i=document.querySelectorAll("main *, header *, section *, form *, article *"),o=null,s=25;for(let r of Array.from(i).slice(0,250)){if(r.id===t||r.closest(`#${t}`))continue;let a=r.getBoundingClientRect();if(a.width<16||a.height<12)continue;let u=n==="vertical"?a.left:a.top,p=Math.round(u-e),d=Math.abs(p);d>=1&&d<s&&(s=d,o={selector:E(r),elementPath:_(r,4),deltaPx:p,rect:{x:Math.round(a.x),y:Math.round(a.y),width:Math.round(a.width),height:Math.round(a.height)}})}return o}var L=`
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
`;var O="data-troupe-outbox";var T=class{host;sessionId=null;sessionPromise=null;bridgeNonce=null;reqSeq=0;pendingOutbox=new Map;constructor(e){this.host=e,this.captureBridgeNonce()}captureBridgeNonce(){let e=window;if(typeof e.__TERMUL_AGENTATION_NONCE__=="string"&&e.__TERMUL_AGENTATION_NONCE__){this.bridgeNonce=e.__TERMUL_AGENTATION_NONCE__;try{delete e.__TERMUL_AGENTATION_NONCE__}catch{}}}getEndpoint(){return window.__TERMUL_AGENTATION_ENDPOINT__}getTabId(){return window.__TERMUL_TAB_ID__}preferHostBridge(){return!!this.bridgeNonce&&window.location.protocol==="https:"}handleOutboxAck(e){if(!e.reqId)return;let t=this.pendingOutbox.get(e.reqId);t&&(this.pendingOutbox.delete(e.reqId),window.clearTimeout(t.timer),e.sessionId&&(this.sessionId=e.sessionId),t.resolve({ok:!!e.ok,id:e.id,sessionId:e.sessionId,error:e.error}))}destroy(){for(let[,e]of this.pendingOutbox)window.clearTimeout(e.timer),e.resolve({ok:!1,error:"Overlay unmounted"});this.pendingOutbox.clear()}sendViaHostBridge(e,t){if(this.captureBridgeNonce(),!this.bridgeNonce)return Promise.resolve({ok:!1,error:"Host bridge unavailable"});let i=`ov-${Date.now()}-${++this.reqSeq}`,o={reqId:i,nonce:this.bridgeNonce,kind:e,url:window.location.href,sessionId:this.sessionId??void 0,payload:t},s=[],r=this.host.getAttribute(O);if(r)try{let a=JSON.parse(r);Array.isArray(a)&&(s=a)}catch{s=[]}return s.push(o),this.host.setAttribute(O,JSON.stringify(s)),new Promise(a=>{let u=window.setTimeout(()=>{this.pendingOutbox.delete(i),a({ok:!1,error:"Host bridge timed out"})},6e3);this.pendingOutbox.set(i,{resolve:a,timer:u})})}async ensureHttpSession(){if(this.sessionId)return this.sessionId;let e=window.__TERMUL_AGENTATION_SESSION_ID__;if(e)return this.sessionId=e,e;let t=this.getEndpoint();return t?(this.sessionPromise||(this.sessionPromise=(async()=>{try{let i=await fetch(`${t}/sessions`,{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({url:window.location.href})});if(!i.ok)return null;let o=await i.json();if(o.id)return this.sessionId=o.id,o.id}catch{}return null})().finally(()=>{this.sessionId||(this.sessionPromise=null)})),this.sessionPromise):null}async postAnnotation(e){this.captureBridgeNonce();let t={url:window.location.href,timestamp:Date.now(),tabId:this.getTabId(),viewportWidth:window.innerWidth,...e};if(this.preferHostBridge()){let o=await this.sendViaHostBridge("annotation",t);if(o.ok)return{ok:!0,id:o.id}}let i=this.getEndpoint();if(i){let o=await this.ensureHttpSession();if(o)try{let s=await fetch(`${i}/sessions/${o}/annotations`,{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify(t)});if(s.ok)return{ok:!0,id:(await s.json().catch(()=>null))?.id}}catch{}}if(!this.preferHostBridge()&&this.bridgeNonce){let o=await this.sendViaHostBridge("annotation",t);if(o.ok)return{ok:!0,id:o.id}}return!i&&!this.bridgeNonce?{ok:!0}:{ok:!1}}async postAction(e){this.captureBridgeNonce();let t=JSON.stringify({viewportWidth:window.innerWidth,...e,tabId:this.getTabId()});if(this.preferHostBridge()&&(await this.sendViaHostBridge("action",{output:t})).ok)return!0;let i=this.getEndpoint();if(i){let o=await this.ensureHttpSession();if(o)try{if((await fetch(`${i}/sessions/${o}/action`,{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({sessionId:o,tabId:this.getTabId(),annotations:[],output:t,timestamp:new Date().toISOString()})})).ok)return!0}catch{}}return!this.preferHostBridge()&&this.bridgeNonce?(await this.sendViaHostBridge("action",{output:t})).ok:!1}};function P(n){let e=n.axis==="vertical"?`x ${n.pos}px`:`y ${n.pos}px`;return typeof n.deltaPx=="number"&&n.deltaPx!==0?`${e} \xB7 ${n.deltaPx>0?`+${n.deltaPx}`:n.deltaPx}px off`:e}function I(n){return Math.max(68,Math.ceil(n.length*7.2+18))}function H(n,e,t){let i=new Map,o=Math.max(160,e),s=Math.max(120,t),r=n.filter(d=>d.axis==="vertical").sort((d,l)=>d.pos-l.pos),a=[];for(let d of r){let l=P(d),x=I(l),h=Math.max(8,Math.min(o-x-8,d.pos+8)),c=a.findIndex(g=>g+6<=h);c===-1?(c=a.length,a.push(h+x)):a[c]=h+x,i.set(d.id,{id:d.id,axis:"vertical",text:l,left:h,bottom:14+c*24})}let u=n.filter(d=>d.axis==="horizontal").sort((d,l)=>d.pos-l.pos),p=[];for(let d of u){let l=P(d),x=I(l),h=Math.max(8,Math.min(s-28,d.pos+6)),c=p.findIndex(m=>m+24<=h);c===-1?(c=p.length,p.push(h)):p[c]=h;let g=Math.max(8,o-x-8),b=Math.min(g,16+c*116);i.set(d.id,{id:d.id,axis:"horizontal",text:l,left:b,top:h})}return i}function M(n,e,t,i){let o=null;for(let s of n){if(s.axis!==e||s.id===i)continue;let r=Math.abs(Math.round(t-s.pos));(o===null||r<o)&&(o=r)}return o}var A="termul-agentation-host",$="troupe-annotate-pause-animations",D=2e3,N=class{host;shadow;layer;hoverBox;selectedBox;dragBox;previewGuideLine;previewGuideBadge;pinsContainer;guidesContainer;popoverEl=null;popoverGuideId=null;popoverTextarea=null;transport;active=!1;tool="comment";pinsHidden=!1;pins=[];guides=[];dragStart=null;lastPointer=null;shiftHeld=!1;draggingGuideId=null;guideReopenTimer=null;activeGuideDragCleanup=null;onWindowKeyDown=null;onWindowKeyUp=null;constructor(){let e=document.getElementById(A);e&&e.remove(),this.host=document.createElement("div"),this.host.id=A,document.body.appendChild(this.host),this.shadow=this.host.attachShadow({mode:"open"}),this.transport=new T(this.host);let t=document.createElement("style");t.textContent=L,this.shadow.appendChild(t),this.layer=document.createElement("div"),this.layer.className="layer",this.shadow.appendChild(this.layer),this.hoverBox=document.createElement("div"),this.hoverBox.className="hover-box",this.shadow.appendChild(this.hoverBox),this.selectedBox=document.createElement("div"),this.selectedBox.className="selected-box",this.shadow.appendChild(this.selectedBox),this.dragBox=document.createElement("div"),this.dragBox.className="drag-box",this.shadow.appendChild(this.dragBox),this.previewGuideLine=document.createElement("div"),this.previewGuideLine.className="guide-preview vertical",this.shadow.appendChild(this.previewGuideLine),this.previewGuideBadge=document.createElement("div"),this.previewGuideBadge.className="guide-preview-badge",this.shadow.appendChild(this.previewGuideBadge),this.guidesContainer=document.createElement("div"),this.shadow.appendChild(this.guidesContainer),this.pinsContainer=document.createElement("div"),this.shadow.appendChild(this.pinsContainer),this.host.style.display="none",this.bindEvents()}hideHoverVisuals(){this.hoverBox.style.display="none"}hideRulerPreview(){this.previewGuideLine.style.display="none",this.previewGuideBadge.style.display="none"}showSelectedBox(e,t){this.selectedBox.className=t?"selected-box rect":"selected-box",this.selectedBox.style.left=`${e.x}px`,this.selectedBox.style.top=`${e.y}px`,this.selectedBox.style.width=`${e.width}px`,this.selectedBox.style.height=`${e.height}px`,this.selectedBox.style.display="block"}hideSelectedBox(){this.selectedBox.style.display="none"}clearGuideReopenTimer(){this.guideReopenTimer!==null&&(window.clearTimeout(this.guideReopenTimer),this.guideReopenTimer=null)}updateRulerPreview(e,t,i){if(!this.active||this.tool!=="ruler"||this.popoverEl||this.draggingGuideId||this.pinsHidden){this.hideRulerPreview();return}let o=i?"horizontal":"vertical",s=Math.round(o==="vertical"?e:t);this.layer.className=o==="vertical"?"layer ruler-v":"layer ruler-h",this.previewGuideLine.className=`guide-preview ${o}`,this.previewGuideLine.style.display="block",o==="vertical"?(this.previewGuideLine.style.left=`${s}px`,this.previewGuideLine.style.top=""):(this.previewGuideLine.style.top=`${s}px`,this.previewGuideLine.style.left="");let r=M(this.guides,o,s),a=o==="vertical"?`V \xB7 x ${s}px`:`H \xB7 y ${s}px`,u=r!==null?` \xB7 \u0394 ${r}px`:"",d=`${a}${u}${o==="vertical"?" \xB7 Shift: H":" \xB7 Release Shift: V"}`;this.previewGuideBadge.textContent=d,this.previewGuideBadge.style.display="block";let l=Math.max(150,Math.ceil(d.length*6.8+18)),x=Math.max(8,Math.min(window.innerWidth-l-8,e+14)),h=Math.max(8,Math.min(window.innerHeight-30,t+14));this.previewGuideBadge.style.left=`${x}px`,this.previewGuideBadge.style.top=`${h}px`}dispatch(e){switch(e.type){case"outbox_ack":this.transport.handleOutboxAck(e);break;case"set_active":this.transport.captureBridgeNonce(),this.active=!!e.active,this.clearGuideReopenTimer(),this.active?(this.host.style.display="",this.renderPinsAndGuides()):(this.activeGuideDragCleanup?.(),this.closePopover(),this.hideHoverVisuals(),this.hideRulerPreview(),this.dragStart=null,this.dragBox.style.display="none",this.host.style.display="none"),this.syncLayerMode();break;case"set_tool":e.tool&&(this.tool=e.tool),this.clearGuideReopenTimer(),this.closePopover(),this.hideHoverVisuals(),this.hideRulerPreview(),this.dragStart=null,this.dragBox.style.display="none",this.syncLayerMode();break;case"set_pause":this.setAnimationsPaused(!!e.paused);break;case"set_pins_hidden":this.pinsHidden=!!e.hidden,this.pinsHidden&&(this.clearGuideReopenTimer(),this.hideHoverVisuals(),this.hideRulerPreview()),this.renderPinsAndGuides();break;case"place_guide":if(e.axis){let t=e.axis==="vertical"?Math.round(window.innerWidth/3):64;this.addGuide(e.axis,t)}break;case"clear":if(this.clearGuideReopenTimer(),e.ids&&e.ids.length>0){let t=new Set(e.ids);this.pins=this.pins.filter(i=>!t.has(i.id)).map((i,o)=>({...i,num:o+1}))}else this.pins=[],this.guides=[],this.closePopover();this.renderPinsAndGuides();break}}setAnimationsPaused(e){let t=document.getElementById($);if(e&&!t){let i=document.createElement("style");i.id=$,i.textContent="*, *::before, *::after { animation-play-state: paused !important; transition-duration: 0.001ms !important; }",document.head.appendChild(i)}else!e&&t&&t.remove()}syncLayerMode(){this.layer.className="layer",this.active&&(this.tool==="rectangle"||this.tool==="screenshot-area"?this.layer.classList.add("capture"):this.tool==="ruler"?this.layer.classList.add(this.shiftHeld?"ruler-h":"ruler-v"):this.layer.classList.add("comment-mode"))}elementUnderPoint(e,t){this.host.style.display="none";let i=document.elementFromPoint(e,t);return this.host.style.display="",!i||i===document.documentElement||i===document.body?null:i}bindEvents(){this.layer.addEventListener("pointermove",e=>{if(this.lastPointer={x:e.clientX,y:e.clientY},this.shiftHeld=e.shiftKey,!this.active||this.popoverEl||this.draggingGuideId)return;if(this.dragStart&&(this.tool==="rectangle"||this.tool==="screenshot-area")){let o=Math.min(this.dragStart.x,e.clientX),s=Math.min(this.dragStart.y,e.clientY),r=Math.abs(e.clientX-this.dragStart.x),a=Math.abs(e.clientY-this.dragStart.y);this.dragBox.style.display="block",this.dragBox.style.left=`${o}px`,this.dragBox.style.top=`${s}px`,this.dragBox.style.width=`${r}px`,this.dragBox.style.height=`${a}px`;return}if(this.tool==="ruler"){this.hideHoverVisuals(),this.updateRulerPreview(e.clientX,e.clientY,e.shiftKey);return}if(this.hideRulerPreview(),this.tool==="screenshot-area"||this.tool==="rectangle"){this.hideHoverVisuals();return}let t=this.elementUnderPoint(e.clientX,e.clientY);if(!t){this.hideHoverVisuals();return}let i=t.getBoundingClientRect();this.hoverBox.style.display="block",this.hoverBox.style.left=`${i.left}px`,this.hoverBox.style.top=`${i.top}px`,this.hoverBox.style.width=`${i.width}px`,this.hoverBox.style.height=`${i.height}px`}),this.layer.addEventListener("pointerleave",()=>{this.hideHoverVisuals(),this.hideRulerPreview()}),this.layer.addEventListener("pointerdown",e=>{if(!(!this.active||e.button!==0)){if(this.tool==="rectangle"||this.tool==="screenshot-area"){this.dragStart={x:e.clientX,y:e.clientY},this.dragBox.style.display="none";return}if(this.tool==="ruler"){this.clearGuideReopenTimer();let t=e.shiftKey?"horizontal":"vertical",i=Math.round(t==="vertical"?e.clientX:e.clientY),o=this.addGuide(t,i);this.hideRulerPreview(),this.openGuidePopover(o,e.clientX,e.clientY);return}if(this.tool==="comment"){let t=this.elementUnderPoint(e.clientX,e.clientY);if(!t)return;let i=R(t);this.openCommentPopover({title:i.element,x:e.clientX,y:e.clientY,kind:"feedback",summary:i})}}}),this.layer.addEventListener("pointerup",e=>{if(!this.dragStart)return;let t=Math.min(this.dragStart.x,e.clientX),i=Math.min(this.dragStart.y,e.clientY),o=Math.abs(e.clientX-this.dragStart.x),s=Math.abs(e.clientY-this.dragStart.y);if(this.dragStart=null,this.dragBox.style.display="none",o<8||s<8)return;let r=window.innerWidth,a={x:Math.round(t),y:Math.round(i),width:Math.round(o),height:Math.round(s)};if(this.tool==="screenshot-area"){requestAnimationFrame(()=>{this.transport.postAction({kind:"screenshot_area",rect:a,dpr:window.devicePixelRatio||1,viewportWidth:r})});return}this.openCommentPopover({title:`Rectangle \xB7 ${a.width}\xD7${a.height}px`,x:a.x,y:a.y,kind:"rectangle",rect:a,viewportWidth:r})}),this.onWindowKeyDown=e=>{if(this.active){if(e.key==="Shift"&&!this.shiftHeld&&(this.shiftHeld=!0,this.tool==="ruler"&&this.lastPointer&&!this.popoverEl&&!this.draggingGuideId&&this.updateRulerPreview(this.lastPointer.x,this.lastPointer.y,!0)),e.key==="Escape"){if(this.clearGuideReopenTimer(),this.popoverEl){this.closePopover();return}this.tool==="screenshot-area"&&(this.dragStart=null,this.dragBox.style.display="none",this.transport.postAction({kind:"screenshot_area_cancelled"}));return}this.popoverEl||(this.tool==="ruler"&&(e.key==="h"||e.key==="H")?this.addGuide("horizontal",Math.round(window.innerHeight/4)):this.tool==="ruler"&&(e.key==="v"||e.key==="V")&&this.addGuide("vertical",32))}},this.onWindowKeyUp=e=>{e.key==="Shift"&&this.shiftHeld&&(this.shiftHeld=!1,this.active&&this.tool==="ruler"&&this.lastPointer&&!this.popoverEl&&!this.draggingGuideId&&this.updateRulerPreview(this.lastPointer.x,this.lastPointer.y,!1))},window.addEventListener("keydown",this.onWindowKeyDown),window.addEventListener("keyup",this.onWindowKeyUp)}addGuide(e,t){let i=k(e,t,A),o={id:`guide-${Date.now()}-${Math.random().toString(36).slice(2,6)}`,axis:e,pos:t,deltaPx:i?.deltaPx,targetSelector:i?.selector};return this.guides.push(o),this.renderPinsAndGuides(),o}startGuideDrag(e,t,i,o){if(o.button!==0)return;o.stopPropagation(),o.preventDefault(),this.activeGuideDragCleanup?.(),this.clearGuideReopenTimer();let s=e.axis==="vertical"?o.clientX:o.clientY,r=e.pos,a=!1,u=l=>{let h=(e.axis==="vertical"?l.clientX:l.clientY)-s;if(!a&&Math.abs(h)<2)return;a||(a=!0,this.draggingGuideId=e.id,this.hideRulerPreview(),this.clearGuideReopenTimer(),this.closePopover(!0),t.classList.add("dragging"),i.classList.add("dragging"));let c=e.axis==="vertical"?window.innerWidth:window.innerHeight,g=Math.max(0,Math.min(c,Math.round(r+h)));e.pos=g,e.axis==="vertical"?(t.style.left=`${g}px`,i.style.left=`${Math.max(8,Math.min(window.innerWidth-130,g+8))}px`):(t.style.top=`${g}px`,i.style.top=`${Math.max(8,Math.min(window.innerHeight-28,g+6))}px`),i.textContent=P({axis:e.axis,pos:g})},p=()=>{window.removeEventListener("pointermove",u),window.removeEventListener("pointerup",d),window.removeEventListener("pointercancel",d),t.classList.remove("dragging"),i.classList.remove("dragging"),this.draggingGuideId===e.id&&(this.draggingGuideId=null),this.activeGuideDragCleanup===p&&(this.activeGuideDragCleanup=null)},d=l=>{if(p(),!a){this.openGuidePopover(e,l.clientX,l.clientY);return}let x=k(e.axis,e.pos,A);e.deltaPx=x?.deltaPx,e.targetSelector=x?.selector,this.renderPinsAndGuides(),this.clearGuideReopenTimer();let h=e.axis==="vertical"?e.pos:l.clientX,c=e.axis==="horizontal"?e.pos:l.clientY,g=e.id;this.guideReopenTimer=window.setTimeout(()=>{if(this.guideReopenTimer=null,!this.active||this.pinsHidden||this.draggingGuideId)return;let b=this.guides.find(m=>m.id===g);b&&this.openGuidePopover(b,h,c)},D)};this.activeGuideDragCleanup=p,window.addEventListener("pointermove",u),window.addEventListener("pointerup",d),window.addEventListener("pointercancel",d)}renderPinsAndGuides(){if(this.pinsContainer.innerHTML="",this.guidesContainer.innerHTML="",this.pinsHidden)return;let e=H(this.guides,window.innerWidth,window.innerHeight);for(let t of this.guides){let i=document.createElement("div");i.className=`guide-hit ${t.axis}`,i.dataset.guideId=t.id,t.axis==="vertical"?i.style.left=`${t.pos}px`:i.style.top=`${t.pos}px`;let o=document.createElement("div");o.className="guide-line";let s=document.createElement("div");s.className="guide-handle";for(let u=0;u<3;u+=1){let p=document.createElement("span");p.className="guide-handle-dot",s.appendChild(p)}i.append(o,s);let r=document.createElement("div");r.className=`guide-badge ${t.axis}`,r.dataset.guideId=t.id;let a=e.get(t.id);r.textContent=a?.text??P(t),a&&(r.style.left=`${a.left}px`,a.bottom!==void 0?r.style.bottom=`${a.bottom}px`:a.top!==void 0&&(r.style.top=`${a.top}px`)),i.addEventListener("pointerenter",()=>this.hideRulerPreview()),r.addEventListener("pointerenter",()=>this.hideRulerPreview()),i.addEventListener("pointerdown",u=>this.startGuideDrag(t,i,r,u)),r.addEventListener("pointerdown",u=>this.startGuideDrag(t,i,r,u)),this.guidesContainer.append(i,r)}for(let t of this.pins){if(t.rect){let o=document.createElement("div");o.className="saved-rect",o.style.left=`${t.rect.x}px`,o.style.top=`${t.rect.y}px`,o.style.width=`${t.rect.width}px`,o.style.height=`${t.rect.height}px`,this.pinsContainer.appendChild(o)}let i=document.createElement("div");i.className=t.kind==="guide"?"pin guide-pin":"pin",i.textContent=String(t.num),i.title=`${t.element}: ${t.comment}`,i.style.left=`${t.x}px`,i.style.top=`${t.y}px`,this.pinsContainer.appendChild(i)}}closePopover(e=!1){if(e&&this.popoverGuideId&&this.popoverTextarea){let t=this.guides.find(i=>i.id===this.popoverGuideId);t&&(t.draftComment=this.popoverTextarea.value)}this.popoverEl&&(this.popoverEl.remove(),this.popoverEl=null),this.popoverGuideId=null,this.popoverTextarea=null,this.hideSelectedBox()}openGuidePopover(e,t,i){let o=`${e.axis==="vertical"?"Vertical":"Horizontal"} guide \xB7 ${e.axis==="vertical"?"x":"y"} = ${e.pos}px`,s=M(this.guides,e.axis,e.pos,e.id);this.openCommentPopover({title:o,x:e.axis==="vertical"?e.pos:t,y:e.axis==="horizontal"?e.pos:i,kind:"guide",guide:e,guideGapPx:s??void 0})}openCommentPopover(e){this.closePopover(),this.hideHoverVisuals(),this.hideRulerPreview();let t=e.rect??e.summary?.boundingBox;t&&t.width>0&&t.height>0&&this.showSelectedBox(t,e.kind==="rectangle");let{pop:i,textarea:o}=B(e,{onCancel:()=>{this.clearGuideReopenTimer(),this.closePopover()},onRemoveGuide:s=>{this.clearGuideReopenTimer(),this.guides=this.guides.filter(r=>r.id!==s),this.closePopover(),this.renderPinsAndGuides()},onSwitchToSpacing:s=>this.openCommentPopover(s),onHideSelectionForCapture:()=>{let s=this.selectedBox.style.display!=="none";return this.selectedBox.style.display="none",s},onRestoreSelectionAfterError:s=>{s&&(this.selectedBox.style.display="block")},onSubmit:s=>this.transport.postAnnotation(s),onSaved:(s,r,a,u,p)=>{let d=this.pins.length+1;this.pins.push({id:r??`pin-${Date.now()}-${d}`,num:d,kind:e.kind,x:u,y:p,rect:e.rect,comment:a,element:e.summary?.element??e.title}),e.guide&&(e.guide.draftComment=void 0),this.popoverEl===s&&this.closePopover(),this.renderPinsAndGuides()}});this.shadow.appendChild(i),this.popoverEl=i,this.popoverGuideId=e.guide?.id??null,this.popoverTextarea=o,o.focus(),requestAnimationFrame(()=>o.focus())}isConnected(){return this.host.isConnected}destroy(){this.clearGuideReopenTimer(),this.activeGuideDragCleanup?.(),this.transport.destroy(),this.setAnimationsPaused(!1),this.onWindowKeyDown&&(window.removeEventListener("keydown",this.onWindowKeyDown),this.onWindowKeyDown=null),this.onWindowKeyUp&&(window.removeEventListener("keyup",this.onWindowKeyUp),this.onWindowKeyUp=null),this.host.remove();let e=window;e.__TROUPE_ANNOTATE_INSTANCE__===this&&(delete e.__TROUPE_ANNOTATE_INSTANCE__,delete e.__TROUPE_ANNOTATE__)}};function G(){let n=window,e=n.__TROUPE_ANNOTATE_INSTANCE__;if(n.__TROUPE_ANNOTATE__&&e?.isConnected())return e;let t=new N;return n.__TROUPE_ANNOTATE_INSTANCE__=t,n.__TROUPE_ANNOTATE__={dispatch:i=>t.dispatch(i),unmount:()=>t.destroy()},n.TermulAgentation={mount:()=>{G().dispatch({type:"set_active",active:!0})},unmount:()=>n.__TROUPE_ANNOTATE_INSTANCE__?.destroy()},t}function F(){typeof document>"u"||(document.readyState==="loading"?document.addEventListener("DOMContentLoaded",()=>{G()}):G())}F();return Y(Z);})();
