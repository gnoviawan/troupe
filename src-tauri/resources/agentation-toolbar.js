"use strict";var AgentationToolbar=(()=>{var P=Object.defineProperty;var O=Object.getOwnPropertyDescriptor;var G=Object.getOwnPropertyNames;var B=Object.prototype.hasOwnProperty;var I=(o,e)=>{for(var n in e)P(o,n,{get:e[n],enumerable:!0})},R=(o,e,n,t)=>{if(e&&typeof e=="object"||typeof e=="function")for(let i of G(e))!B.call(o,i)&&i!==n&&P(o,i,{get:()=>e[i],enumerable:!(t=O(e,i))||t.enumerable});return o};var D=o=>R(P({},"__esModule",{value:!0}),o);var Y={};I(Y,{buildElementPath:()=>v,buildShortLabel:()=>b});var H=["display","position","width","height","padding","margin","gap","font-size","line-height","font-weight","border-radius"];function b(o){let e=o.tagName.toLowerCase();if(o.id)return`${e}#${o.id}`;let n=typeof o.className=="string"?o.className.trim().split(/\s+/).filter(Boolean):[];return n.length>0?`${e}.${n.slice(0,2).join(".")}`:e}function v(o,e=4){let n=[],t=o;for(;t&&t!==document.body&&n.length<e&&(n.unshift(b(t)),!t.id);)t=t.parentElement;return n.join(" > ")||o.tagName.toLowerCase()}function z(o){let e=Object.keys(o).find(a=>a.startsWith("__reactFiber$")||a.startsWith("__reactInternalInstance$"));if(!e)return;let n=[],t=o[e],i=0;for(;t&&n.length<5&&i<40;){i+=1;let a=t.type,s=typeof a=="function"||typeof a=="object"&&a!==null?a.displayName||a.name:void 0;s&&/^[A-Z]/.test(s)&&!n.includes(s)&&n.push(s),t=t.return}return n.length>0?n.join(" < "):void 0}function U(o){let e=window.getComputedStyle(o),n=[];for(let t of H){let i=e.getPropertyValue(t);i&&i!=="normal"&&i!=="none"&&i!=="0px"&&i!=="static"&&n.push(`${t}: ${i}`)}return n.join("; ")}function _(o){let e=o.getBoundingClientRect(),n=(o.textContent??"").trim().replace(/\s+/g," ").slice(0,120),t=typeof o.className=="string"&&o.className.trim()?o.className.trim().slice(0,200):void 0;return{element:b(o),elementPath:v(o,4),fullPath:v(o,10),cssClasses:t,nearbyText:n||void 0,computedStyles:U(o)||void 0,reactComponents:z(o),boundingBox:{x:Math.round(e.x),y:Math.round(e.y),width:Math.round(e.width),height:Math.round(e.height)}}}function A(o,e,n){let t=document.querySelectorAll("main *, header *, section *, form *, article *"),i=null,a=25;for(let s of Array.from(t).slice(0,250)){if(s.id===n||s.closest(`#${n}`))continue;let r=s.getBoundingClientRect();if(r.width<16||r.height<12)continue;let x=o==="vertical"?r.left:r.top,l=Math.round(x-e),c=Math.abs(l);c>=1&&c<a&&(a=c,i={selector:b(s),elementPath:v(s,4),deltaPx:l,rect:{x:Math.round(r.x),y:Math.round(r.y),width:Math.round(r.width),height:Math.round(r.height)}})}return i}var S=`
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
  white-space: nowrap;
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
`;var T="termul-agentation-host",L="troupe-annotate-pause-animations",C=class{host;shadow;layer;hoverBox;dragBox;spacingPill;spacingTopLine;spacingLeftLine;pinsContainer;guidesContainer;popoverEl=null;active=!1;tool="comment";pinsHidden=!1;pins=[];guides=[];sessionId=null;sessionPromise=null;dragStart=null;onWindowKeyDown=null;constructor(){let e=document.getElementById(T);e&&e.remove(),this.host=document.createElement("div"),this.host.id=T,document.body.appendChild(this.host),this.shadow=this.host.attachShadow({mode:"open"});let n=document.createElement("style");n.textContent=S,this.shadow.appendChild(n),this.layer=document.createElement("div"),this.layer.className="layer",this.shadow.appendChild(this.layer),this.hoverBox=document.createElement("div"),this.hoverBox.className="hover-box",this.shadow.appendChild(this.hoverBox),this.dragBox=document.createElement("div"),this.dragBox.className="drag-box",this.shadow.appendChild(this.dragBox),this.spacingPill=document.createElement("div"),this.spacingPill.className="spacing-pill",this.shadow.appendChild(this.spacingPill),this.spacingTopLine=document.createElement("div"),this.spacingTopLine.className="spacing-line v",this.shadow.appendChild(this.spacingTopLine),this.spacingLeftLine=document.createElement("div"),this.spacingLeftLine.className="spacing-line h",this.shadow.appendChild(this.spacingLeftLine),this.guidesContainer=document.createElement("div"),this.shadow.appendChild(this.guidesContainer),this.pinsContainer=document.createElement("div"),this.shadow.appendChild(this.pinsContainer),this.host.style.display="none",this.bindEvents()}getEndpoint(){return window.__TERMUL_AGENTATION_ENDPOINT__}getTabId(){return window.__TERMUL_TAB_ID__}async ensureSession(){if(this.sessionId)return this.sessionId;let e=window.__TERMUL_AGENTATION_SESSION_ID__;if(e)return this.sessionId=e,e;let n=this.getEndpoint();return n?(this.sessionPromise||(this.sessionPromise=(async()=>{try{let t=await fetch(`${n}/sessions`,{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({url:window.location.href})});if(!t.ok)return null;let i=await t.json();if(i.id)return this.sessionId=i.id,i.id}catch{}return null})().finally(()=>{this.sessionId||(this.sessionPromise=null)})),this.sessionPromise):null}async postAnnotation(e){let n=this.getEndpoint();if(!n)return{ok:!0};let t=await this.ensureSession();if(!t)return{ok:!1};try{let i=await fetch(`${n}/sessions/${t}/annotations`,{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({url:window.location.href,timestamp:Date.now(),tabId:this.getTabId(),viewportWidth:window.innerWidth,...e})});return i.ok?{ok:!0,id:(await i.json().catch(()=>null))?.id}:{ok:!1}}catch{return{ok:!1}}}async postAction(e){let n=this.getEndpoint();if(!n)return;let t=await this.ensureSession();if(t)try{await fetch(`${n}/sessions/${t}/action`,{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({sessionId:t,tabId:this.getTabId(),annotations:[],output:JSON.stringify({...e,tabId:this.getTabId(),viewportWidth:window.innerWidth}),timestamp:new Date().toISOString()})})}catch{}}hideHoverVisuals(){this.hoverBox.style.display="none",this.spacingPill.style.display="none",this.spacingTopLine.style.display="none",this.spacingLeftLine.style.display="none"}dispatch(e){switch(e.type){case"set_active":this.active=!!e.active,this.active?(this.host.style.display="",this.renderPinsAndGuides()):(this.closePopover(),this.hideHoverVisuals(),this.host.style.display="none"),this.syncLayerMode();break;case"set_tool":e.tool&&(this.tool=e.tool),this.closePopover(),this.hideHoverVisuals(),this.syncLayerMode();break;case"set_pause":this.setAnimationsPaused(!!e.paused);break;case"set_pins_hidden":this.pinsHidden=!!e.hidden,this.renderPinsAndGuides();break;case"place_guide":if(e.axis){let n=e.axis==="vertical"?Math.round(window.innerWidth/3):64;this.addGuide(e.axis,n)}break;case"clear":if(e.ids&&e.ids.length>0){let n=new Set(e.ids);this.pins=this.pins.filter(t=>!n.has(t.id)).map((t,i)=>({...t,num:i+1}))}else this.pins=[],this.guides=[],this.closePopover();this.renderPinsAndGuides();break}}setAnimationsPaused(e){let n=document.getElementById(L);if(e&&!n){let t=document.createElement("style");t.id=L,t.textContent="*, *::before, *::after { animation-play-state: paused !important; transition-duration: 0.001ms !important; }",document.head.appendChild(t)}else!e&&n&&n.remove()}syncLayerMode(){this.layer.className="layer",this.active&&(this.tool==="rectangle"||this.tool==="screenshot-area"?this.layer.classList.add("capture"):this.layer.classList.add("comment-mode"))}elementUnderPoint(e,n){this.host.style.display="none";let t=document.elementFromPoint(e,n);return this.host.style.display="",!t||t===document.documentElement||t===document.body?null:t}bindEvents(){this.layer.addEventListener("pointermove",e=>{if(!this.active||this.popoverEl)return;if(this.dragStart&&(this.tool==="rectangle"||this.tool==="screenshot-area")){let i=Math.min(this.dragStart.x,e.clientX),a=Math.min(this.dragStart.y,e.clientY),s=Math.abs(e.clientX-this.dragStart.x),r=Math.abs(e.clientY-this.dragStart.y);this.dragBox.style.display="block",this.dragBox.style.left=`${i}px`,this.dragBox.style.top=`${a}px`,this.dragBox.style.width=`${s}px`,this.dragBox.style.height=`${r}px`;return}let n=this.elementUnderPoint(e.clientX,e.clientY);if(!n){this.hideHoverVisuals();return}let t=n.getBoundingClientRect();if(this.hoverBox.style.display="block",this.hoverBox.style.left=`${t.left}px`,this.hoverBox.style.top=`${t.top}px`,this.hoverBox.style.width=`${t.width}px`,this.hoverBox.style.height=`${t.height}px`,this.tool==="ruler"){let i=Math.round(t.left+t.width/2),a=Math.round(t.top+t.height/2),s=Math.max(0,Math.round(t.top)),r=Math.max(0,Math.round(t.left));this.spacingTopLine.style.display=s>0?"block":"none",this.spacingTopLine.style.left=`${i}px`,this.spacingTopLine.style.top="0px",this.spacingTopLine.style.height=`${s}px`,this.spacingLeftLine.style.display=r>0?"block":"none",this.spacingLeftLine.style.left="0px",this.spacingLeftLine.style.top=`${a}px`,this.spacingLeftLine.style.width=`${r}px`,this.spacingPill.style.display="block",this.spacingPill.textContent=`${Math.round(t.width)}\xD7${Math.round(t.height)}px \xB7 \u2191${s} \u2190${r} \xB7 Alt+click measure`,this.spacingPill.style.left=`${Math.min(window.innerWidth-280,Math.max(8,t.left))}px`,this.spacingPill.style.top=`${Math.max(8,t.top-24)}px`}else this.spacingPill.style.display="none",this.spacingTopLine.style.display="none",this.spacingLeftLine.style.display="none"}),this.layer.addEventListener("pointerdown",e=>{if(!(!this.active||e.button!==0)){if(this.tool==="rectangle"||this.tool==="screenshot-area"){this.dragStart={x:e.clientX,y:e.clientY},this.dragBox.style.display="none";return}if(this.tool==="ruler"){let n=this.elementUnderPoint(e.clientX,e.clientY),t=n?(()=>{let r=n.getBoundingClientRect(),x=_(n),l=Math.max(0,Math.round(r.top)),c=Math.max(0,Math.round(r.left)),h=e.shiftKey?"horizontal":"vertical",f=h==="vertical"?l:c;return{summary:x,axis:h,positionPx:Math.round(h==="vertical"?r.top:r.left),gapPx:f}})():void 0;if(e.altKey&&t){this.openCommentPopover({title:`Spacing \xB7 ${t.gapPx}px (${t.summary.element})`,x:e.clientX,y:e.clientY,kind:"measurement",summary:t.summary,measurement:{axis:t.axis,positionPx:t.positionPx,gapPx:t.gapPx}});return}let i=e.shiftKey?"horizontal":"vertical",a=Math.round(i==="vertical"?e.clientX:e.clientY),s=this.addGuide(i,a);this.openGuidePopover(s,e.clientX,e.clientY,t);return}if(this.tool==="comment"){let n=this.elementUnderPoint(e.clientX,e.clientY);if(!n)return;let t=_(n);this.openCommentPopover({title:t.element,x:e.clientX,y:e.clientY,kind:"feedback",summary:t})}}}),this.layer.addEventListener("pointerup",e=>{if(!this.dragStart)return;let n=Math.min(this.dragStart.x,e.clientX),t=Math.min(this.dragStart.y,e.clientY),i=Math.abs(e.clientX-this.dragStart.x),a=Math.abs(e.clientY-this.dragStart.y);if(this.dragStart=null,this.dragBox.style.display="none",i<8||a<8)return;let s={x:Math.round(n),y:Math.round(t),width:Math.round(i),height:Math.round(a)};if(this.tool==="screenshot-area"){this.postAction({kind:"screenshot_area",rect:s,dpr:window.devicePixelRatio||1});return}this.openCommentPopover({title:`Rectangle \xB7 ${s.width}\xD7${s.height}px`,x:s.x,y:s.y,kind:"rectangle",rect:s})}),this.onWindowKeyDown=e=>{if(this.active){if(e.key==="Escape"){this.closePopover();return}this.popoverEl||(this.tool==="ruler"&&(e.key==="h"||e.key==="H")?this.addGuide("horizontal",Math.round(window.innerHeight/4)):this.tool==="ruler"&&(e.key==="v"||e.key==="V")&&this.addGuide("vertical",32))}},window.addEventListener("keydown",this.onWindowKeyDown)}addGuide(e,n){let t=A(e,n,T),i={id:`guide-${Date.now()}-${Math.random().toString(36).slice(2,6)}`,axis:e,pos:n,deltaPx:t?.deltaPx,targetSelector:t?.selector};return this.guides.push(i),this.renderPinsAndGuides(),i}renderPinsAndGuides(){this.pinsContainer.innerHTML="",this.guidesContainer.innerHTML="";for(let e of this.guides){let n=document.createElement("div");n.className=`guide ${e.axis}`,e.axis==="vertical"?n.style.left=`${e.pos}px`:n.style.top=`${e.pos}px`,n.addEventListener("click",s=>{s.stopPropagation(),this.openGuidePopover(e,s.clientX,s.clientY)}),this.guidesContainer.appendChild(n);let t=document.createElement("div");t.className="guide-badge";let i=e.axis==="vertical"?`x ${e.pos}px`:`y ${e.pos}px`,a=typeof e.deltaPx=="number"&&e.deltaPx!==0?` \xB7 ${e.deltaPx>0?`+${e.deltaPx}`:e.deltaPx}px off`:"";t.textContent=`${i}${a}`,e.axis==="vertical"?(t.style.left=`${Math.min(window.innerWidth-130,e.pos+6)}px`,t.style.bottom="14px"):(t.style.top=`${Math.min(window.innerHeight-28,e.pos+6)}px`,t.style.left="16px"),t.addEventListener("click",s=>{s.stopPropagation(),this.openGuidePopover(e,s.clientX,s.clientY)}),this.guidesContainer.appendChild(t)}if(!this.pinsHidden)for(let e of this.pins){if(e.rect){let t=document.createElement("div");t.className="saved-rect",t.style.left=`${e.rect.x}px`,t.style.top=`${e.rect.y}px`,t.style.width=`${e.rect.width}px`,t.style.height=`${e.rect.height}px`,this.pinsContainer.appendChild(t)}let n=document.createElement("div");n.className=e.kind==="guide"?"pin guide-pin":"pin",n.textContent=String(e.num),n.title=`${e.element}: ${e.comment}`,n.style.left=`${e.x}px`,n.style.top=`${e.y}px`,this.pinsContainer.appendChild(n)}}closePopover(){this.popoverEl&&(this.popoverEl.remove(),this.popoverEl=null)}openGuidePopover(e,n,t,i){let a=`${e.axis==="vertical"?"Vertical":"Horizontal"} guide \xB7 ${e.axis==="vertical"?"x":"y"} = ${e.pos}px`;this.openCommentPopover({title:a,x:e.axis==="vertical"?e.pos:n,y:e.axis==="horizontal"?e.pos:t,kind:"guide",guide:e,spacingCandidate:i})}openCommentPopover(e){this.closePopover(),this.hideHoverVisuals();let n=document.createElement("div");n.className="popover";let t=Math.min(332,Math.max(240,window.innerWidth-24)),i=Math.max(12,Math.min(window.innerWidth-t-12,e.x+14)),a=Math.max(12,Math.min(window.innerHeight-190,e.y+14));n.style.left=`${i}px`,n.style.top=`${a}px`;let s=document.createElement("div");s.className="pop-head";let r=document.createElement("span");r.className="pop-title",r.textContent=e.title;let x=document.createElement("span");x.className="pop-pill",x.textContent="Auto-crop on add",s.append(r,x);let l=document.createElement("textarea");l.className="pop-input",l.placeholder=e.kind==="guide"?`Describe alignment change at ${e.title}\u2026`:"What should change on this element\u2026";let c=document.createElement("div");if(c.className="pop-foot",e.guide){let d=document.createElement("div");d.className="pop-links";let u=document.createElement("button");if(u.type="button",u.className="pop-link",u.textContent="Remove guide",u.addEventListener("click",()=>{this.guides=this.guides.filter(p=>p.id!==e.guide?.id),this.closePopover(),this.renderPinsAndGuides()}),d.appendChild(u),e.spacingCandidate){let p=e.spacingCandidate,g=document.createElement("button");g.type="button",g.className="pop-link measure",g.textContent=`Save spacing (${p.gapPx}px)`,g.addEventListener("click",()=>{this.guides=this.guides.filter(w=>w.id!==e.guide?.id),this.renderPinsAndGuides(),this.openCommentPopover({title:`Spacing \xB7 ${p.gapPx}px (${p.summary.element})`,x:e.x,y:e.y,kind:"measurement",summary:p.summary,measurement:{axis:p.axis,positionPx:p.positionPx,gapPx:p.gapPx}})}),d.appendChild(g)}c.appendChild(d)}else{let d=document.createElement("span");d.className="pop-hint";let u=/mac/i.test(navigator.platform||navigator.userAgent);d.textContent=u?"\u2318Enter to add":"Ctrl+Enter to add",c.appendChild(d)}let h=document.createElement("div");h.className="pop-error",h.style.display="none";let f=document.createElement("div");f.className="pop-actions";let y=document.createElement("button");y.type="button",y.className="btn btn-sec",y.textContent="Cancel",y.addEventListener("click",()=>this.closePopover());let m=document.createElement("button");m.type="button",m.className="btn btn-pri",m.textContent="Add comment";let E=!1,N=async()=>{if(E)return;let d=l.value.trim();if(!d)return;E=!0,m.disabled=!0,m.textContent="Saving\u2026",h.style.display="none";let u=Math.round(e.x),p=Math.round(e.y),g={x:u,y:p,comment:d,element:e.summary?.element??e.title,elementPath:e.summary?.elementPath??e.guide?.targetSelector??e.title,fullPath:e.summary?.fullPath,cssClasses:e.summary?.cssClasses,nearbyText:e.summary?.nearbyText,computedStyles:e.summary?.computedStyles,reactComponents:e.summary?.reactComponents,boundingBox:e.rect??e.summary?.boundingBox,kind:e.kind};e.guide?g.measurement={axis:e.guide.axis,positionPx:e.guide.pos,toSelector:e.guide.targetSelector,deltaPx:e.guide.deltaPx}:e.measurement&&(g.measurement={axis:e.measurement.axis,positionPx:e.measurement.positionPx,toSelector:e.summary?.elementPath,gapPx:e.measurement.gapPx}),n.style.display="none";let w=await this.postAnnotation(g);if(E=!1,!w.ok){n.style.display="flex",m.disabled=!1,m.textContent="Retry",h.textContent="Could not save annotation. Try again.",h.style.display="block";return}let M=this.pins.length+1,$={id:w.id??`pin-${Date.now()}-${M}`,num:M,kind:e.kind,x:u,y:p,rect:e.rect,comment:d,element:e.summary?.element??e.title};this.pins.push($),this.popoverEl===n&&this.closePopover(),this.renderPinsAndGuides()};m.addEventListener("click",()=>{N()}),l.addEventListener("keydown",d=>{(d.metaKey||d.ctrlKey)&&d.key==="Enter"&&(d.preventDefault(),N())}),f.append(y,m),c.appendChild(f),n.append(s,l,h,c),this.shadow.appendChild(n),this.popoverEl=n,l.focus(),requestAnimationFrame(()=>l.focus())}isConnected(){return this.host.isConnected}destroy(){this.setAnimationsPaused(!1),this.onWindowKeyDown&&(window.removeEventListener("keydown",this.onWindowKeyDown),this.onWindowKeyDown=null),this.host.remove();let e=window;e.__TROUPE_ANNOTATE_INSTANCE__===this&&(delete e.__TROUPE_ANNOTATE_INSTANCE__,delete e.__TROUPE_ANNOTATE__)}};function k(){let o=window,e=o.__TROUPE_ANNOTATE_INSTANCE__;if(o.__TROUPE_ANNOTATE__&&e?.isConnected())return e;let n=new C;return o.__TROUPE_ANNOTATE_INSTANCE__=n,o.__TROUPE_ANNOTATE__={dispatch:t=>n.dispatch(t),unmount:()=>n.destroy()},o.TermulAgentation={mount:()=>{k().dispatch({type:"set_active",active:!0})},unmount:()=>o.__TROUPE_ANNOTATE_INSTANCE__?.destroy()},n}function W(){document.readyState==="loading"?document.addEventListener("DOMContentLoaded",()=>{k()}):k()}W();return D(Y);})();
