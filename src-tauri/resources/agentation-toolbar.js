"use strict";var AgentationToolbar=(()=>{var S=Object.defineProperty;var X=Object.getOwnPropertyDescriptor;var z=Object.getOwnPropertyNames;var W=Object.prototype.hasOwnProperty;var U=(o,e)=>{for(var t in e)S(o,t,{get:e[t],enumerable:!0})},V=(o,e,t,i)=>{if(e&&typeof e=="object"||typeof e=="function")for(let n of z(e))!W.call(o,n)&&n!==t&&S(o,n,{get:()=>e[n],enumerable:!(i=X(e,n))||i.enumerable});return o};var Y=o=>V(S({},"__esModule",{value:!0}),o);var J={};U(J,{GUIDE_POPOVER_REOPEN_DELAY_MS:()=>D,TroupeAnnotationOverlay:()=>N,buildElementPath:()=>E,buildShortLabel:()=>_,mountOverlay:()=>C});function I(o,e){let t=document.createElement("div");t.className="popover";let i=Math.min(332,Math.max(240,window.innerWidth-24)),n=Math.max(12,Math.min(window.innerWidth-i-12,o.x+14)),s=Math.max(12,Math.min(window.innerHeight-190,o.y+14));t.style.left=`${n}px`,t.style.top=`${s}px`;let r=document.createElement("div");r.className="pop-head";let a=document.createElement("span");a.className="pop-title",a.textContent=o.title;let h=document.createElement("span");h.className="pop-pill",h.textContent="Auto-crop on add",r.append(a,h);let d=document.createElement("textarea");d.className="pop-input",o.guide?.draftComment&&(d.value=o.guide.draftComment),d.placeholder=o.kind==="guide"?`Describe alignment change at ${o.title}\u2026`:"What should change on this element\u2026";let l=document.createElement("div");if(l.className="pop-foot",o.guide){let u=o.guide,f=document.createElement("div");f.className="pop-links";let y=document.createElement("button");if(y.type="button",y.className="pop-link",y.textContent="Remove guide",y.addEventListener("click",()=>e.onRemoveGuide(u.id)),f.appendChild(y),typeof o.guideGapPx=="number"&&o.guideGapPx>0){let v=o.guideGapPx,w=document.createElement("button");w.type="button",w.className="pop-link measure",w.textContent=`Save spacing (${v}px)`,w.addEventListener("click",()=>{e.onSwitchToSpacing({title:`Spacing \xB7 ${v}px (${u.axis})`,x:o.x,y:o.y,kind:"measurement",measurement:{axis:u.axis,positionPx:u.pos,gapPx:v}})}),f.appendChild(w)}l.appendChild(f)}else{let u=document.createElement("span");u.className="pop-hint";let f=/mac/i.test(navigator.platform||navigator.userAgent);u.textContent=f?"\u2318Enter to add":"Ctrl+Enter to add",l.appendChild(u)}let p=document.createElement("div");p.className="pop-error",p.style.display="none";let x=document.createElement("div");x.className="pop-actions";let g=document.createElement("button");g.type="button",g.className="btn btn-sec",g.textContent="Cancel",g.addEventListener("click",()=>e.onCancel());let m=document.createElement("button");m.type="button",m.className="btn btn-pri",m.textContent="Add comment";let c=!1,b=async()=>{if(c)return;let u=d.value.trim();if(!u)return;c=!0,m.disabled=!0,m.textContent="Saving\u2026",p.style.display="none";let f=Math.round(o.x),y=Math.round(o.y),v={x:f,y,comment:u,element:o.summary?.element??o.title,elementPath:o.summary?.elementPath??o.guide?.targetSelector??o.title,fullPath:o.summary?.fullPath,cssClasses:o.summary?.cssClasses,nearbyText:o.summary?.nearbyText,computedStyles:o.summary?.computedStyles,reactComponents:o.summary?.reactComponents,boundingBox:o.rect??o.summary?.boundingBox,viewportWidth:o.viewportWidth??window.innerWidth,kind:o.kind};o.guide?v.measurement={axis:o.guide.axis,positionPx:o.guide.pos,toSelector:o.guide.targetSelector,deltaPx:o.guide.deltaPx}:o.measurement&&(v.measurement={axis:o.measurement.axis,positionPx:o.measurement.positionPx,toSelector:o.summary?.elementPath,gapPx:o.measurement.gapPx});let w=e.onHideSelectionForCapture();t.style.display="none";let M=await e.onSubmit(v,{comment:u,pinX:f,pinY:y});if(c=!1,!M.ok){if(!t.isConnected)return;t.style.display="flex",e.onRestoreSelectionAfterError(t,w),m.disabled=!1,m.textContent="Retry",p.textContent="Could not save annotation. Try again.",p.style.display="block";return}e.onSaved(t,M.id,u,f,y)};return m.addEventListener("click",()=>{b()}),d.addEventListener("keydown",u=>{(u.metaKey||u.ctrlKey)&&u.key==="Enter"&&(u.preventDefault(),b())}),x.append(g,m),l.appendChild(x),t.append(r,d,p,l),{pop:t,textarea:d}}var K=["display","position","width","height","padding","margin","gap","font-size","line-height","font-weight","border-radius"];function _(o){let e=o.tagName.toLowerCase();if(o.id)return`${e}#${o.id}`;let t=typeof o.className=="string"?o.className.trim().split(/\s+/).filter(Boolean):[];return t.length>0?`${e}.${t.slice(0,2).join(".")}`:e}function E(o,e=4){let t=[],i=o;for(;i&&i!==document.body&&t.length<e&&(t.unshift(_(i)),!i.id);)i=i.parentElement;return t.join(" > ")||o.tagName.toLowerCase()}function j(o){let e=Object.keys(o).find(s=>s.startsWith("__reactFiber$")||s.startsWith("__reactInternalInstance$"));if(!e)return;let t=[],i=o[e],n=0;for(;i&&t.length<5&&n<40;){n+=1;let s=i.type,r=typeof s=="function"||typeof s=="object"&&s!==null?s.displayName||s.name:void 0;r&&/^[A-Z]/.test(r)&&!t.includes(r)&&t.push(r),i=i.return}return t.length>0?t.join(" < "):void 0}function q(o){let e=window.getComputedStyle(o),t=[];for(let i of K){let n=e.getPropertyValue(i);n&&n!=="normal"&&n!=="none"&&n!=="0px"&&n!=="static"&&t.push(`${i}: ${n}`)}return t.join("; ")}function R(o){let e=o.getBoundingClientRect(),t=(o.textContent??"").trim().replace(/\s+/g," ").slice(0,120),i=typeof o.className=="string"&&o.className.trim()?o.className.trim().slice(0,200):void 0;return{element:_(o),elementPath:E(o,4),fullPath:E(o,10),cssClasses:i,nearbyText:t||void 0,computedStyles:q(o)||void 0,reactComponents:j(o),boundingBox:{x:Math.round(e.x),y:Math.round(e.y),width:Math.round(e.width),height:Math.round(e.height)}}}function k(o,e,t){let i=document.querySelectorAll("main *, header *, section *, form *, article *"),n=null,s=25;for(let r of Array.from(i).slice(0,250)){if(r.id===t||r.closest(`#${t}`))continue;let a=r.getBoundingClientRect();if(a.width<16||a.height<12)continue;let h=o==="vertical"?a.left:a.top,d=Math.round(h-e),l=Math.abs(d);l>=1&&l<s&&(s=l,n={selector:_(r),elementPath:E(r,4),deltaPx:d,rect:{x:Math.round(a.x),y:Math.round(a.y),width:Math.round(a.width),height:Math.round(a.height)}})}return n}var L=`
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
.guides-layer.passthrough .guide-hit,
.guides-layer.passthrough .guide-badge,
.pins-layer.passthrough .pin {
  pointer-events: none;
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
`;var P="data-troupe-outbox";var A=class{host;sessionId=null;sessionPromise=null;bridgeSessionInit=null;bridgeNonce=null;reqSeq=0;pendingOutbox=new Map;constructor(e,t){this.host=e,this.setBridgeNonce(t)}setBridgeNonce(e){typeof e=="string"&&e.length>0&&(this.bridgeNonce=e)}getEndpoint(){return window.__TERMUL_AGENTATION_ENDPOINT__}getTabId(){return window.__TERMUL_TAB_ID__}preferHostBridge(){return!!this.bridgeNonce&&window.location.protocol==="https:"}handleOutboxAck(e){if(!e.reqId)return;let t=this.pendingOutbox.get(e.reqId);t&&(this.pendingOutbox.delete(e.reqId),window.clearTimeout(t.timer),e.sessionId&&(this.sessionId=e.sessionId),t.resolve({ok:!!e.ok,id:e.id,sessionId:e.sessionId,error:e.error}))}removeFromOutbox(e){let t=this.host.getAttribute(P);if(!t)return!1;try{let i=JSON.parse(t);if(!Array.isArray(i))return!1;let n=i.filter(s=>!(s&&typeof s=="object"&&s.reqId===e));return n.length===i.length?!1:(n.length===0?this.host.removeAttribute(P):this.host.setAttribute(P,JSON.stringify(n)),!0)}catch{return!1}}destroy(){for(let[e,t]of this.pendingOutbox)window.clearTimeout(t.timer),this.removeFromOutbox(e),t.resolve({ok:!1,error:"Overlay unmounted"});this.pendingOutbox.clear(),this.bridgeSessionInit=null}async sendViaHostBridge(e,t){if(!this.bridgeNonce)return{ok:!1,error:"Host bridge unavailable",drained:!1};for(;!this.sessionId&&this.bridgeSessionInit;)await this.bridgeSessionInit;let i=null;if(!this.sessionId){let d=new Promise(l=>{i=l});this.bridgeSessionInit=d}let n=`ov-${Date.now()}-${++this.reqSeq}`,s={reqId:n,nonce:this.bridgeNonce,kind:e,url:window.location.href,sessionId:this.sessionId??void 0,payload:t},r=[],a=this.host.getAttribute(P);if(a)try{let d=JSON.parse(a);Array.isArray(d)&&(r=d)}catch{r=[]}r.push(s),this.host.setAttribute(P,JSON.stringify(r));let h=await new Promise(d=>{let l=window.setTimeout(()=>{this.pendingOutbox.delete(n);let p=this.removeFromOutbox(n);d({ok:!1,error:"Host bridge timed out",drained:!p})},6e3);this.pendingOutbox.set(n,{resolve:p=>d({...p,drained:!1}),timer:l})});return i&&(this.bridgeSessionInit=null,i()),h}async ensureHttpSession(){if(this.sessionId)return this.sessionId;let e=window.__TERMUL_AGENTATION_SESSION_ID__;if(e)return this.sessionId=e,e;let t=this.getEndpoint();return t?(this.sessionPromise||(this.sessionPromise=(async()=>{try{let i=await fetch(`${t}/sessions`,{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({url:window.location.href})});if(!i.ok)return null;let n=await i.json();if(n.id)return this.sessionId=n.id,n.id}catch{}return null})().finally(()=>{this.sessionId||(this.sessionPromise=null)})),this.sessionPromise):null}async postAnnotation(e){let t={url:window.location.href,timestamp:Date.now(),tabId:this.getTabId(),viewportWidth:window.innerWidth,...e};if(this.preferHostBridge()){let n=await this.sendViaHostBridge("annotation",t);if(n.ok)return{ok:!0,id:n.id};if(n.drained)return{ok:!1}}let i=this.getEndpoint();if(i){let n=await this.ensureHttpSession();if(n)try{let s=await fetch(`${i}/sessions/${n}/annotations`,{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify(t)});if(s.ok)return{ok:!0,id:(await s.json().catch(()=>null))?.id}}catch{}}if(!this.preferHostBridge()&&this.bridgeNonce){let n=await this.sendViaHostBridge("annotation",t);if(n.ok)return{ok:!0,id:n.id}}return!i&&!this.bridgeNonce?{ok:!0}:{ok:!1}}async postAction(e){let t=JSON.stringify({viewportWidth:window.innerWidth,...e,tabId:this.getTabId()});if(this.preferHostBridge()){let n=await this.sendViaHostBridge("action",{output:t});if(n.ok)return!0;if(n.drained)return!1}let i=this.getEndpoint();if(i){let n=await this.ensureHttpSession();if(n)try{if((await fetch(`${i}/sessions/${n}/action`,{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({sessionId:n,tabId:this.getTabId(),annotations:[],output:t,timestamp:new Date().toISOString()})})).ok)return!0}catch{}}return!this.preferHostBridge()&&this.bridgeNonce?(await this.sendViaHostBridge("action",{output:t})).ok:!1}};function T(o){let e=o.axis==="vertical"?`x ${o.pos}px`:`y ${o.pos}px`;return typeof o.deltaPx=="number"&&o.deltaPx!==0?`${e} \xB7 ${o.deltaPx>0?`+${o.deltaPx}`:o.deltaPx}px off`:e}function O(o){return Math.max(68,Math.ceil(o.length*7.2+18))}function H(o,e,t){let i=new Map,n=Math.max(160,e),s=Math.max(120,t),r=o.filter(l=>l.axis==="vertical").sort((l,p)=>l.pos-p.pos),a=[];for(let l of r){let p=T(l),x=O(p),g=Math.max(8,Math.min(n-x-8,l.pos+8)),m=a.findIndex(c=>c+6<=g);m===-1?(m=a.length,a.push(g+x)):a[m]=g+x,i.set(l.id,{id:l.id,axis:"vertical",text:p,left:g,bottom:14+m*24})}let h=o.filter(l=>l.axis==="horizontal").sort((l,p)=>l.pos-p.pos),d=[];for(let l of h){let p=T(l),x=O(p),g=Math.max(8,Math.min(s-28,l.pos+6)),m=Math.max(8,n-x-8),c=d.findIndex(f=>f.left<=m&&f.bottomEdge+24<=g),b,u;if(c!==-1)b=d[c].left,u=g,d[c].bottomEdge=u;else{let f=16+d.length*116;if(d.length===0||f<=m&&f>=d[d.length-1].left+68+6)b=Math.min(m,f),u=g,d.push({left:b,bottomEdge:u});else{c=0;for(let v=1;v<d.length;v+=1)d[v].bottomEdge<d[c].bottomEdge&&(c=v);b=Math.min(m,d[c].left),u=Math.min(s-28,Math.max(g,d[c].bottomEdge+24)),d[c].bottomEdge=u}}i.set(l.id,{id:l.id,axis:"horizontal",text:p,left:b,top:u})}return i}function B(o,e,t,i){let n=null;for(let s of o){if(s.axis!==e||s.id===i)continue;let r=Math.abs(Math.round(t-s.pos));(n===null||r<n)&&(n=r)}return n}var G="termul-agentation-host",$="troupe-annotate-pause-animations",D=2e3,N=class{host;shadow;layer;hoverBox;selectedBox;dragBox;previewGuideLine;previewGuideBadge;pinsContainer;guidesContainer;popoverEl=null;popoverGuideId=null;popoverTextarea=null;transport;active=!1;tool="comment";pinsHidden=!1;captureHidden=!1;selectedRect=null;pins=[];guides=[];dragStart=null;lastPointer=null;shiftHeld=!1;draggingGuideId=null;guideReopenTimer=null;activeGuideDragCleanup=null;onWindowKeyDown=null;onWindowKeyUp=null;constructor(e){let t=document.getElementById(G);t&&t.remove(),this.host=document.createElement("div"),this.host.id=G,document.body.appendChild(this.host),this.shadow=this.host.attachShadow({mode:"open"}),this.transport=new A(this.host,e);let i=document.createElement("style");i.textContent=L,this.shadow.appendChild(i),this.layer=document.createElement("div"),this.layer.className="layer",this.shadow.appendChild(this.layer),this.hoverBox=document.createElement("div"),this.hoverBox.className="hover-box",this.shadow.appendChild(this.hoverBox),this.selectedBox=document.createElement("div"),this.selectedBox.className="selected-box",this.shadow.appendChild(this.selectedBox),this.dragBox=document.createElement("div"),this.dragBox.className="drag-box",this.shadow.appendChild(this.dragBox),this.previewGuideLine=document.createElement("div"),this.previewGuideLine.className="guide-preview vertical",this.shadow.appendChild(this.previewGuideLine),this.previewGuideBadge=document.createElement("div"),this.previewGuideBadge.className="guide-preview-badge",this.shadow.appendChild(this.previewGuideBadge),this.guidesContainer=document.createElement("div"),this.guidesContainer.className="guides-layer",this.shadow.appendChild(this.guidesContainer),this.pinsContainer=document.createElement("div"),this.pinsContainer.className="pins-layer",this.shadow.appendChild(this.pinsContainer),this.host.style.display="none",this.bindEvents()}setBridgeNonce(e){this.transport.setBridgeNonce(e)}syncHostVisibility(){this.host.style.display=this.active&&!this.captureHidden?"":"none"}hideHoverVisuals(){this.hoverBox.style.display="none"}hideRulerPreview(){this.previewGuideLine.style.display="none",this.previewGuideBadge.style.display="none"}showSelectedBox(e,t){this.selectedRect={rect:e,isRectangle:t},this.selectedBox.className=t?"selected-box rect":"selected-box",this.selectedBox.style.left=`${e.x}px`,this.selectedBox.style.top=`${e.y}px`,this.selectedBox.style.width=`${e.width}px`,this.selectedBox.style.height=`${e.height}px`,this.selectedBox.style.display=this.pinsHidden?"none":"block"}hideSelectedBox(){this.selectedRect=null,this.selectedBox.style.display="none"}clearGuideReopenTimer(){this.guideReopenTimer!==null&&(window.clearTimeout(this.guideReopenTimer),this.guideReopenTimer=null)}updateRulerPreview(e,t,i){if(!this.active||this.tool!=="ruler"||this.popoverEl||this.draggingGuideId||this.pinsHidden){this.hideRulerPreview();return}let n=i?"horizontal":"vertical",s=Math.round(n==="vertical"?e:t);this.layer.className=n==="vertical"?"layer ruler-v":"layer ruler-h",this.previewGuideLine.className=`guide-preview ${n}`,this.previewGuideLine.style.display="block",n==="vertical"?(this.previewGuideLine.style.left=`${s}px`,this.previewGuideLine.style.top=""):(this.previewGuideLine.style.top=`${s}px`,this.previewGuideLine.style.left="");let r=B(this.guides,n,s),a=n==="vertical"?`V \xB7 x ${s}px`:`H \xB7 y ${s}px`,h=r!==null?` \xB7 \u0394 ${r}px`:"",l=`${a}${h}${n==="vertical"?" \xB7 Shift: H":" \xB7 Release Shift: V"}`;this.previewGuideBadge.textContent=l,this.previewGuideBadge.style.display="block";let p=Math.max(150,Math.ceil(l.length*6.8+18)),x=Math.max(8,Math.min(window.innerWidth-p-8,e+14)),g=Math.max(8,Math.min(window.innerHeight-30,t+14));this.previewGuideBadge.style.left=`${x}px`,this.previewGuideBadge.style.top=`${g}px`}dispatch(e){switch(e.type){case"outbox_ack":this.transport.handleOutboxAck(e);break;case"set_active":this.active=!!e.active,this.captureHidden=!1,this.clearGuideReopenTimer(),this.active?(this.syncHostVisibility(),this.renderPinsAndGuides()):(this.activeGuideDragCleanup?.(),this.closePopover(),this.hideHoverVisuals(),this.hideRulerPreview(),this.dragStart=null,this.dragBox.style.display="none",this.syncHostVisibility()),this.syncLayerMode();break;case"set_tool":e.tool&&(this.tool=e.tool),this.clearGuideReopenTimer(),this.closePopover(),this.hideHoverVisuals(),this.hideRulerPreview(),this.dragStart=null,this.dragBox.style.display="none",this.syncLayerMode();break;case"set_pause":this.setAnimationsPaused(!!e.paused);break;case"set_pins_hidden":this.pinsHidden=!!e.hidden,this.pinsHidden?(this.clearGuideReopenTimer(),this.hideHoverVisuals(),this.hideRulerPreview(),this.selectedBox.style.display="none",this.dragBox.style.display="none"):this.popoverEl&&this.selectedRect&&this.showSelectedBox(this.selectedRect.rect,this.selectedRect.isRectangle),this.renderPinsAndGuides();break;case"set_capture_hidden":this.captureHidden=!!e.hidden,this.syncHostVisibility();break;case"place_guide":if(e.axis){let t=e.axis==="vertical"?Math.round(window.innerWidth/3):64;this.addGuide(e.axis,t)}break;case"clear":if(this.clearGuideReopenTimer(),e.ids&&e.ids.length>0){let t=new Set(e.ids);this.pins=this.pins.filter(i=>!t.has(i.id)).map((i,n)=>({...i,num:n+1}))}else this.pins=[],this.guides=[],this.closePopover();this.renderPinsAndGuides();break}}setAnimationsPaused(e){let t=document.getElementById($);if(e&&!t){let i=document.createElement("style");i.id=$,i.textContent="*, *::before, *::after { animation-play-state: paused !important; transition-duration: 0.001ms !important; }",document.head.appendChild(i)}else!e&&t&&t.remove()}syncLayerMode(){this.layer.className="layer";let e=this.active&&(this.tool==="rectangle"||this.tool==="screenshot-area");this.guidesContainer.classList.toggle("passthrough",e),this.pinsContainer.classList.toggle("passthrough",e),this.active&&(this.tool==="rectangle"||this.tool==="screenshot-area"?this.layer.classList.add("capture"):this.tool==="ruler"?this.layer.classList.add(this.shiftHeld?"ruler-h":"ruler-v"):this.layer.classList.add("comment-mode"))}elementUnderPoint(e,t){this.host.style.display="none";let i=document.elementFromPoint(e,t);return this.syncHostVisibility(),!i||i===document.documentElement||i===document.body?null:i}bindEvents(){this.layer.addEventListener("pointermove",e=>{if(this.lastPointer={x:e.clientX,y:e.clientY},this.shiftHeld=e.shiftKey,!this.active||this.popoverEl||this.draggingGuideId)return;if(this.dragStart&&(this.tool==="rectangle"||this.tool==="screenshot-area")){if(this.pinsHidden){this.dragBox.style.display="none";return}let n=Math.min(this.dragStart.x,e.clientX),s=Math.min(this.dragStart.y,e.clientY),r=Math.abs(e.clientX-this.dragStart.x),a=Math.abs(e.clientY-this.dragStart.y);this.dragBox.style.display="block",this.dragBox.style.left=`${n}px`,this.dragBox.style.top=`${s}px`,this.dragBox.style.width=`${r}px`,this.dragBox.style.height=`${a}px`;return}if(this.tool==="ruler"){this.hideHoverVisuals(),this.updateRulerPreview(e.clientX,e.clientY,e.shiftKey);return}if(this.hideRulerPreview(),this.tool==="screenshot-area"||this.tool==="rectangle"||this.pinsHidden){this.hideHoverVisuals();return}let t=this.elementUnderPoint(e.clientX,e.clientY);if(!t){this.hideHoverVisuals();return}let i=t.getBoundingClientRect();this.hoverBox.style.display="block",this.hoverBox.style.left=`${i.left}px`,this.hoverBox.style.top=`${i.top}px`,this.hoverBox.style.width=`${i.width}px`,this.hoverBox.style.height=`${i.height}px`}),this.layer.addEventListener("pointerleave",()=>{this.hideHoverVisuals(),this.hideRulerPreview()}),this.layer.addEventListener("pointerdown",e=>{if(!(!this.active||e.button!==0)){if(this.tool==="rectangle"||this.tool==="screenshot-area"){this.dragStart={x:e.clientX,y:e.clientY},this.dragBox.style.display="none";return}if(this.tool==="ruler"){this.clearGuideReopenTimer();let t=e.shiftKey?"horizontal":"vertical",i=Math.round(t==="vertical"?e.clientX:e.clientY),n=this.addGuide(t,i);this.hideRulerPreview(),this.openGuidePopover(n,e.clientX,e.clientY);return}if(this.tool==="comment"){let t=this.elementUnderPoint(e.clientX,e.clientY);if(!t)return;let i=R(t);this.openCommentPopover({title:i.element,x:e.clientX,y:e.clientY,kind:"feedback",summary:i})}}}),this.layer.addEventListener("pointerup",e=>{if(!this.dragStart)return;let t=Math.min(this.dragStart.x,e.clientX),i=Math.min(this.dragStart.y,e.clientY),n=Math.abs(e.clientX-this.dragStart.x),s=Math.abs(e.clientY-this.dragStart.y);if(this.dragStart=null,this.dragBox.style.display="none",n<8||s<8)return;let r=window.innerWidth,a={x:Math.round(t),y:Math.round(i),width:Math.round(n),height:Math.round(s)};if(this.tool==="screenshot-area"){requestAnimationFrame(()=>{this.transport.postAction({kind:"screenshot_area",rect:a,dpr:window.devicePixelRatio||1,viewportWidth:r})});return}this.openCommentPopover({title:`Rectangle \xB7 ${a.width}\xD7${a.height}px`,x:a.x,y:a.y,kind:"rectangle",rect:a,viewportWidth:r})}),this.onWindowKeyDown=e=>{if(this.active){if(e.key==="Shift"&&!this.shiftHeld&&(this.shiftHeld=!0,this.tool==="ruler"&&this.lastPointer&&!this.popoverEl&&!this.draggingGuideId&&this.updateRulerPreview(this.lastPointer.x,this.lastPointer.y,!0)),e.key==="Escape"){if(this.clearGuideReopenTimer(),this.popoverEl){this.closePopover();return}this.tool==="screenshot-area"&&(this.dragStart=null,this.dragBox.style.display="none",this.transport.postAction({kind:"screenshot_area_cancelled"}));return}this.popoverEl||(this.tool==="ruler"&&(e.key==="h"||e.key==="H")?this.addGuide("horizontal",Math.round(window.innerHeight/4)):this.tool==="ruler"&&(e.key==="v"||e.key==="V")&&this.addGuide("vertical",32))}},this.onWindowKeyUp=e=>{e.key==="Shift"&&this.shiftHeld&&(this.shiftHeld=!1,this.active&&this.tool==="ruler"&&this.lastPointer&&!this.popoverEl&&!this.draggingGuideId&&this.updateRulerPreview(this.lastPointer.x,this.lastPointer.y,!1))},window.addEventListener("keydown",this.onWindowKeyDown),window.addEventListener("keyup",this.onWindowKeyUp)}addGuide(e,t){let i=k(e,t,G),n={id:`guide-${Date.now()}-${Math.random().toString(36).slice(2,6)}`,axis:e,pos:t,deltaPx:i?.deltaPx,targetSelector:i?.selector};return this.guides.push(n),this.renderPinsAndGuides(),n}startGuideDrag(e,t,i,n){if(n.button!==0)return;n.stopPropagation(),n.preventDefault(),this.activeGuideDragCleanup?.(),this.clearGuideReopenTimer();let s=e.axis==="vertical"?n.clientX:n.clientY,r=e.pos,a=!1,h=p=>{let g=(e.axis==="vertical"?p.clientX:p.clientY)-s;if(!a&&Math.abs(g)<2)return;a||(a=!0,this.draggingGuideId=e.id,this.hideRulerPreview(),this.clearGuideReopenTimer(),this.closePopover(!0),t.classList.add("dragging"),i.classList.add("dragging"));let m=e.axis==="vertical"?window.innerWidth:window.innerHeight,c=Math.max(0,Math.min(m,Math.round(r+g)));e.pos=c,e.axis==="vertical"?(t.style.left=`${c}px`,i.style.left=`${Math.max(8,Math.min(window.innerWidth-130,c+8))}px`):(t.style.top=`${c}px`,i.style.top=`${Math.max(8,Math.min(window.innerHeight-28,c+6))}px`),i.textContent=T({axis:e.axis,pos:c})},d=()=>{window.removeEventListener("pointermove",h),window.removeEventListener("pointerup",l),window.removeEventListener("pointercancel",l),t.classList.remove("dragging"),i.classList.remove("dragging"),this.draggingGuideId===e.id&&(this.draggingGuideId=null),this.activeGuideDragCleanup===d&&(this.activeGuideDragCleanup=null)},l=p=>{if(d(),!a){this.openGuidePopover(e,p.clientX,p.clientY);return}let x=k(e.axis,e.pos,G);e.deltaPx=x?.deltaPx,e.targetSelector=x?.selector,this.renderPinsAndGuides(),this.clearGuideReopenTimer();let g=e.axis==="vertical"?e.pos:p.clientX,m=e.axis==="horizontal"?e.pos:p.clientY,c=e.id;this.guideReopenTimer=window.setTimeout(()=>{if(this.guideReopenTimer=null,!this.active||this.pinsHidden||this.draggingGuideId)return;let b=this.guides.find(u=>u.id===c);b&&this.openGuidePopover(b,g,m)},D)};this.activeGuideDragCleanup=d,window.addEventListener("pointermove",h),window.addEventListener("pointerup",l),window.addEventListener("pointercancel",l)}renderPinsAndGuides(){if(this.pinsContainer.innerHTML="",this.guidesContainer.innerHTML="",this.pinsHidden)return;let e=H(this.guides,window.innerWidth,window.innerHeight);for(let t of this.guides){let i=document.createElement("div");i.className=`guide-hit ${t.axis}`,i.dataset.guideId=t.id,t.axis==="vertical"?i.style.left=`${t.pos}px`:i.style.top=`${t.pos}px`;let n=document.createElement("div");n.className="guide-line";let s=document.createElement("div");s.className="guide-handle";for(let h=0;h<3;h+=1){let d=document.createElement("span");d.className="guide-handle-dot",s.appendChild(d)}i.append(n,s);let r=document.createElement("div");r.className=`guide-badge ${t.axis}`,r.dataset.guideId=t.id;let a=e.get(t.id);r.textContent=a?.text??T(t),a&&(r.style.left=`${a.left}px`,a.bottom!==void 0?r.style.bottom=`${a.bottom}px`:a.top!==void 0&&(r.style.top=`${a.top}px`)),i.addEventListener("pointerenter",()=>this.hideRulerPreview()),r.addEventListener("pointerenter",()=>this.hideRulerPreview()),i.addEventListener("pointerdown",h=>this.startGuideDrag(t,i,r,h)),r.addEventListener("pointerdown",h=>this.startGuideDrag(t,i,r,h)),this.guidesContainer.append(i,r)}for(let t of this.pins){if(t.rect){let n=document.createElement("div");n.className="saved-rect",n.style.left=`${t.rect.x}px`,n.style.top=`${t.rect.y}px`,n.style.width=`${t.rect.width}px`,n.style.height=`${t.rect.height}px`,this.pinsContainer.appendChild(n)}let i=document.createElement("div");i.className=t.kind==="guide"?"pin guide-pin":"pin",i.textContent=String(t.num),i.title=`${t.element}: ${t.comment}`,i.style.left=`${t.x}px`,i.style.top=`${t.y}px`,this.pinsContainer.appendChild(i)}}closePopover(e=!1){if(e&&this.popoverGuideId&&this.popoverTextarea){let t=this.guides.find(i=>i.id===this.popoverGuideId);t&&(t.draftComment=this.popoverTextarea.value)}this.popoverEl&&(this.popoverEl.remove(),this.popoverEl=null),this.popoverGuideId=null,this.popoverTextarea=null,this.hideSelectedBox()}openGuidePopover(e,t,i){let n=`${e.axis==="vertical"?"Vertical":"Horizontal"} guide \xB7 ${e.axis==="vertical"?"x":"y"} = ${e.pos}px`,s=B(this.guides,e.axis,e.pos,e.id);this.openCommentPopover({title:n,x:e.axis==="vertical"?e.pos:t,y:e.axis==="horizontal"?e.pos:i,kind:"guide",guide:e,guideGapPx:s??void 0})}openCommentPopover(e){this.closePopover(),this.hideHoverVisuals(),this.hideRulerPreview();let t=e.rect??e.summary?.boundingBox;t&&t.width>0&&t.height>0&&this.showSelectedBox(t,e.kind==="rectangle");let{pop:i,textarea:n}=I(e,{onCancel:()=>{this.clearGuideReopenTimer(),this.closePopover()},onRemoveGuide:s=>{this.clearGuideReopenTimer();let r=this.pins.filter(a=>a.guideId===s).map(a=>a.id);if(r.length>0){let a=new Set(r);this.pins=this.pins.filter(h=>!a.has(h.id)).map((h,d)=>({...h,num:d+1})),this.transport.postAction({kind:"annotations_removed",ids:r})}this.guides=this.guides.filter(a=>a.id!==s),this.closePopover(),this.renderPinsAndGuides()},onSwitchToSpacing:s=>this.openCommentPopover(s),onHideSelectionForCapture:()=>{let s=this.selectedBox.style.display!=="none";return this.selectedBox.style.display="none",s},onRestoreSelectionAfterError:(s,r)=>{r&&s.isConnected&&this.popoverEl===s&&!this.pinsHidden&&(this.selectedBox.style.display="block")},onSubmit:s=>this.transport.postAnnotation(s),onSaved:(s,r,a,h,d)=>{let l=this.pins.length+1;this.pins.push({id:r??`pin-${Date.now()}-${l}`,num:l,kind:e.kind,x:h,y:d,rect:e.rect,guideId:e.guide?.id,comment:a,element:e.summary?.element??e.title}),e.guide&&(e.guide.draftComment=void 0),this.popoverEl===s&&this.closePopover(),this.renderPinsAndGuides()}});this.shadow.appendChild(i),this.popoverEl=i,this.popoverGuideId=e.guide?.id??null,this.popoverTextarea=n,n.focus(),requestAnimationFrame(()=>n.focus())}isConnected(){return this.host.isConnected}destroy(){this.clearGuideReopenTimer(),this.activeGuideDragCleanup?.(),this.transport.destroy(),this.setAnimationsPaused(!1),this.onWindowKeyDown&&(window.removeEventListener("keydown",this.onWindowKeyDown),this.onWindowKeyDown=null),this.onWindowKeyUp&&(window.removeEventListener("keyup",this.onWindowKeyUp),this.onWindowKeyUp=null),this.host.remove();let e=window;e.__TROUPE_ANNOTATE_INSTANCE__===this&&(delete e.__TROUPE_ANNOTATE_INSTANCE__,delete e.__TROUPE_ANNOTATE__)}};function C(o){let e=window,t=e.__TROUPE_ANNOTATE_INSTANCE__;if(e.__TROUPE_ANNOTATE__&&t?.isConnected())return t.setBridgeNonce(o),t;let i=new N(o);return e.__TROUPE_ANNOTATE_INSTANCE__=i,e.__TROUPE_ANNOTATE__={dispatch:n=>i.dispatch(n),unmount:()=>i.destroy()},e.TermulAgentation={mount:()=>{C(o).dispatch({type:"set_active",active:!0})},unmount:()=>e.__TROUPE_ANNOTATE_INSTANCE__?.destroy()},i}function F(){if(typeof document>"u")return;let o=typeof __troupeBridgeNonce=="string"?__troupeBridgeNonce:void 0;document.readyState==="loading"?document.addEventListener("DOMContentLoaded",()=>{C(o)}):C(o)}F();return Y(J);})();
