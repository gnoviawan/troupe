"use strict";var AgentationToolbar=(()=>{var y=Object.defineProperty;var N=Object.getOwnPropertyDescriptor;var S=Object.getOwnPropertyNames;var k=Object.prototype.hasOwnProperty;var L=(i,e)=>{for(var n in e)y(i,n,{get:e[n],enumerable:!0})},A=(i,e,n,t)=>{if(e&&typeof e=="object"||typeof e=="function")for(let o of S(e))!k.call(i,o)&&o!==n&&y(i,o,{get:()=>e[o],enumerable:!(t=N(e,o))||t.enumerable});return i};var $=i=>A(y({},"__esModule",{value:!0}),i);var H={};L(H,{buildElementPath:()=>g,buildShortLabel:()=>x});var O=["display","position","width","height","padding","margin","gap","font-size","line-height","font-weight","border-radius"];function x(i){let e=i.tagName.toLowerCase();if(i.id)return`${e}#${i.id}`;let n=typeof i.className=="string"?i.className.trim().split(/\s+/).filter(Boolean):[];return n.length>0?`${e}.${n.slice(0,2).join(".")}`:e}function g(i,e=4){let n=[],t=i;for(;t&&t!==document.body&&n.length<e&&(n.unshift(x(t)),!t.id);)t=t.parentElement;return n.join(" > ")||i.tagName.toLowerCase()}function B(i){let e=Object.keys(i).find(a=>a.startsWith("__reactFiber$")||a.startsWith("__reactInternalInstance$"));if(!e)return;let n=[],t=i[e],o=0;for(;t&&n.length<5&&o<40;){o+=1;let a=t.type,s=typeof a=="function"||typeof a=="object"&&a!==null?a.displayName||a.name:void 0;s&&/^[A-Z]/.test(s)&&!n.includes(s)&&n.push(s),t=t.return}return n.length>0?n.join(" < "):void 0}function G(i){let e=window.getComputedStyle(i),n=[];for(let t of O){let o=e.getPropertyValue(t);o&&o!=="normal"&&o!=="none"&&o!=="0px"&&o!=="static"&&n.push(`${t}: ${o}`)}return n.join("; ")}function P(i){let e=i.getBoundingClientRect(),n=(i.textContent??"").trim().replace(/\s+/g," ").slice(0,120),t=typeof i.className=="string"&&i.className.trim()?i.className.trim().slice(0,200):void 0;return{element:x(i),elementPath:g(i,4),fullPath:g(i,10),cssClasses:t,nearbyText:n||void 0,computedStyles:G(i)||void 0,reactComponents:B(i),boundingBox:{x:Math.round(e.x),y:Math.round(e.y),width:Math.round(e.width),height:Math.round(e.height)}}}function _(i,e,n){let t=document.querySelectorAll("main *, header *, section *, form *, article *"),o=null,a=25;for(let s of Array.from(t).slice(0,250)){if(s.id===n||s.closest(`#${n}`))continue;let r=s.getBoundingClientRect();if(r.width<16||r.height<12)continue;let l=i==="vertical"?r.left:r.top,p=Math.round(l-e),c=Math.abs(p);c>=1&&c<a&&(a=c,o={selector:x(s),elementPath:g(s,4),deltaPx:p,rect:{x:Math.round(r.x),y:Math.round(r.y),width:Math.round(r.width),height:Math.round(r.height)}})}return o}var T=`
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
  width: 332px;
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
.pop-foot { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
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
`;var v="termul-agentation-host",C="troupe-annotate-pause-animations",b=class{host;shadow;layer;hoverBox;dragBox;spacingPill;spacingTopLine;spacingLeftLine;pinsContainer;guidesContainer;popoverEl=null;active=!1;tool="comment";pinsHidden=!1;pins=[];guides=[];sessionId=null;dragStart=null;constructor(){let e=document.getElementById(v);e&&e.remove(),this.host=document.createElement("div"),this.host.id=v,document.body.appendChild(this.host),this.shadow=this.host.attachShadow({mode:"open"});let n=document.createElement("style");n.textContent=T,this.shadow.appendChild(n),this.layer=document.createElement("div"),this.layer.className="layer",this.shadow.appendChild(this.layer),this.hoverBox=document.createElement("div"),this.hoverBox.className="hover-box",this.shadow.appendChild(this.hoverBox),this.dragBox=document.createElement("div"),this.dragBox.className="drag-box",this.shadow.appendChild(this.dragBox),this.spacingPill=document.createElement("div"),this.spacingPill.className="spacing-pill",this.shadow.appendChild(this.spacingPill),this.spacingTopLine=document.createElement("div"),this.spacingTopLine.className="spacing-line v",this.shadow.appendChild(this.spacingTopLine),this.spacingLeftLine=document.createElement("div"),this.spacingLeftLine.className="spacing-line h",this.shadow.appendChild(this.spacingLeftLine),this.guidesContainer=document.createElement("div"),this.shadow.appendChild(this.guidesContainer),this.pinsContainer=document.createElement("div"),this.shadow.appendChild(this.pinsContainer),this.host.style.display="none",this.bindEvents()}getEndpoint(){return window.__TERMUL_AGENTATION_ENDPOINT__}async ensureSession(){if(this.sessionId)return this.sessionId;let e=window.__TERMUL_AGENTATION_SESSION_ID__;if(e)return this.sessionId=e,e;let n=this.getEndpoint();if(!n)return null;try{let t=await fetch(`${n}/sessions`,{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({url:window.location.href})});if(!t.ok)return null;let o=await t.json();if(o.id)return this.sessionId=o.id,o.id}catch{}return null}async postAnnotation(e){let n=this.getEndpoint();if(!n)return;let t=await this.ensureSession();if(t)try{await fetch(`${n}/sessions/${t}/annotations`,{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({url:window.location.href,timestamp:Date.now(),...e})})}catch{}}async postAction(e){let n=this.getEndpoint();if(!n)return;let t=await this.ensureSession();if(t)try{await fetch(`${n}/sessions/${t}/action`,{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({sessionId:t,annotations:[],output:JSON.stringify(e),timestamp:new Date().toISOString()})})}catch{}}hideHoverVisuals(){this.hoverBox.style.display="none",this.spacingPill.style.display="none",this.spacingTopLine.style.display="none",this.spacingLeftLine.style.display="none"}dispatch(e){switch(e.type){case"set_active":this.active=!!e.active,this.active?(this.host.style.display="",this.renderPinsAndGuides()):(this.closePopover(),this.hideHoverVisuals(),this.host.style.display="none"),this.syncLayerMode();break;case"set_tool":e.tool&&(this.tool=e.tool),this.closePopover(),this.hideHoverVisuals(),this.syncLayerMode();break;case"set_pause":this.setAnimationsPaused(!!e.paused);break;case"set_pins_hidden":this.pinsHidden=!!e.hidden,this.renderPinsAndGuides();break;case"place_guide":if(e.axis){let n=e.axis==="vertical"?Math.round(window.innerWidth/3):64;this.addGuide(e.axis,n)}break;case"clear":this.pins=[],this.guides=[],this.closePopover(),this.renderPinsAndGuides();break}}setAnimationsPaused(e){let n=document.getElementById(C);if(e&&!n){let t=document.createElement("style");t.id=C,t.textContent="*, *::before, *::after { animation-play-state: paused !important; transition-duration: 0.001ms !important; }",document.head.appendChild(t)}else!e&&n&&n.remove()}syncLayerMode(){this.layer.className="layer",this.active&&(this.tool==="rectangle"||this.tool==="screenshot-area"?this.layer.classList.add("capture"):this.layer.classList.add("comment-mode"))}elementUnderPoint(e,n){this.host.style.display="none";let t=document.elementFromPoint(e,n);return this.host.style.display="",!t||t===document.documentElement||t===document.body?null:t}bindEvents(){this.layer.addEventListener("pointermove",e=>{if(!this.active||this.popoverEl)return;if(this.dragStart&&(this.tool==="rectangle"||this.tool==="screenshot-area")){let o=Math.min(this.dragStart.x,e.clientX),a=Math.min(this.dragStart.y,e.clientY),s=Math.abs(e.clientX-this.dragStart.x),r=Math.abs(e.clientY-this.dragStart.y);this.dragBox.style.display="block",this.dragBox.style.left=`${o}px`,this.dragBox.style.top=`${a}px`,this.dragBox.style.width=`${s}px`,this.dragBox.style.height=`${r}px`;return}let n=this.elementUnderPoint(e.clientX,e.clientY);if(!n){this.hideHoverVisuals();return}let t=n.getBoundingClientRect();if(this.hoverBox.style.display="block",this.hoverBox.style.left=`${t.left}px`,this.hoverBox.style.top=`${t.top}px`,this.hoverBox.style.width=`${t.width}px`,this.hoverBox.style.height=`${t.height}px`,this.tool==="ruler"){let o=Math.round(t.left+t.width/2),a=Math.round(t.top+t.height/2),s=Math.max(0,Math.round(t.top)),r=Math.max(0,Math.round(t.left));this.spacingTopLine.style.display=s>0?"block":"none",this.spacingTopLine.style.left=`${o}px`,this.spacingTopLine.style.top="0px",this.spacingTopLine.style.height=`${s}px`,this.spacingLeftLine.style.display=r>0?"block":"none",this.spacingLeftLine.style.left="0px",this.spacingLeftLine.style.top=`${a}px`,this.spacingLeftLine.style.width=`${r}px`,this.spacingPill.style.display="block",this.spacingPill.textContent=`${Math.round(t.width)}\xD7${Math.round(t.height)}px \xB7 \u2191${s} \u2190${r}`,this.spacingPill.style.left=`${Math.min(window.innerWidth-200,Math.max(8,t.left))}px`,this.spacingPill.style.top=`${Math.max(8,t.top-24)}px`}else this.spacingPill.style.display="none",this.spacingTopLine.style.display="none",this.spacingLeftLine.style.display="none"}),this.layer.addEventListener("pointerdown",e=>{if(!(!this.active||e.button!==0)){if(this.tool==="rectangle"||this.tool==="screenshot-area"){this.dragStart={x:e.clientX,y:e.clientY},this.dragBox.style.display="none";return}if(this.tool==="ruler"){let n=e.shiftKey?"horizontal":"vertical",t=Math.round(n==="vertical"?e.clientX:e.clientY),o=this.addGuide(n,t);this.openGuidePopover(o,e.clientX,e.clientY);return}if(this.tool==="comment"){let n=this.elementUnderPoint(e.clientX,e.clientY);if(!n)return;let t=P(n);this.openCommentPopover({title:t.element,x:e.clientX,y:e.clientY,kind:"feedback",summary:t})}}}),this.layer.addEventListener("pointerup",e=>{if(!this.dragStart)return;let n=Math.min(this.dragStart.x,e.clientX),t=Math.min(this.dragStart.y,e.clientY),o=Math.abs(e.clientX-this.dragStart.x),a=Math.abs(e.clientY-this.dragStart.y);if(this.dragStart=null,this.dragBox.style.display="none",o<8||a<8)return;let s={x:Math.round(n),y:Math.round(t),width:Math.round(o),height:Math.round(a)};if(this.tool==="screenshot-area"){this.postAction({kind:"screenshot_area",rect:s,dpr:window.devicePixelRatio||1});return}this.openCommentPopover({title:`Rectangle \xB7 ${s.width}\xD7${s.height}px`,x:s.x,y:s.y,kind:"rectangle",rect:s})}),window.addEventListener("keydown",e=>{if(this.active){if(e.key==="Escape"){this.closePopover();return}this.popoverEl||(this.tool==="ruler"&&(e.key==="h"||e.key==="H")?this.addGuide("horizontal",Math.round(window.innerHeight/4)):this.tool==="ruler"&&(e.key==="v"||e.key==="V")&&this.addGuide("vertical",32))}})}addGuide(e,n){let t=_(e,n,v),o={id:`guide-${Date.now()}-${Math.random().toString(36).slice(2,6)}`,axis:e,pos:n,deltaPx:t?.deltaPx,targetSelector:t?.selector};return this.guides.push(o),this.renderPinsAndGuides(),o}renderPinsAndGuides(){this.pinsContainer.innerHTML="",this.guidesContainer.innerHTML="";for(let e of this.guides){let n=document.createElement("div");n.className=`guide ${e.axis}`,e.axis==="vertical"?n.style.left=`${e.pos}px`:n.style.top=`${e.pos}px`,n.addEventListener("click",s=>{s.stopPropagation(),this.openGuidePopover(e,s.clientX,s.clientY)}),this.guidesContainer.appendChild(n);let t=document.createElement("div");t.className="guide-badge";let o=e.axis==="vertical"?`x ${e.pos}px`:`y ${e.pos}px`,a=typeof e.deltaPx=="number"&&e.deltaPx!==0?` \xB7 ${e.deltaPx>0?`+${e.deltaPx}`:e.deltaPx}px off`:"";t.textContent=`${o}${a}`,e.axis==="vertical"?(t.style.left=`${Math.min(window.innerWidth-130,e.pos+6)}px`,t.style.bottom="14px"):(t.style.top=`${Math.min(window.innerHeight-28,e.pos+6)}px`,t.style.left="16px"),t.addEventListener("click",s=>{s.stopPropagation(),this.openGuidePopover(e,s.clientX,s.clientY)}),this.guidesContainer.appendChild(t)}if(!this.pinsHidden)for(let e of this.pins){if(e.rect){let t=document.createElement("div");t.className="saved-rect",t.style.left=`${e.rect.x}px`,t.style.top=`${e.rect.y}px`,t.style.width=`${e.rect.width}px`,t.style.height=`${e.rect.height}px`,this.pinsContainer.appendChild(t)}let n=document.createElement("div");n.className=e.kind==="guide"?"pin guide-pin":"pin",n.textContent=String(e.num),n.title=`${e.element}: ${e.comment}`,n.style.left=`${e.x}px`,n.style.top=`${e.y}px`,this.pinsContainer.appendChild(n)}}closePopover(){this.popoverEl&&(this.popoverEl.remove(),this.popoverEl=null)}openGuidePopover(e,n,t){let o=`${e.axis==="vertical"?"Vertical":"Horizontal"} guide \xB7 ${e.axis==="vertical"?"x":"y"} = ${e.pos}px`;this.openCommentPopover({title:o,x:e.axis==="vertical"?e.pos:n,y:e.axis==="horizontal"?e.pos:t,kind:"guide",guide:e})}openCommentPopover(e){this.closePopover(),this.hideHoverVisuals();let n=document.createElement("div");n.className="popover";let t=Math.max(12,Math.min(window.innerWidth-348,e.x+14)),o=Math.max(12,Math.min(window.innerHeight-190,e.y+14));n.style.left=`${t}px`,n.style.top=`${o}px`;let a=document.createElement("div");a.className="pop-head";let s=document.createElement("span");s.className="pop-title",s.textContent=e.title;let r=document.createElement("span");r.className="pop-pill",r.textContent="Auto-crop on add",a.append(s,r);let l=document.createElement("textarea");l.className="pop-input",l.placeholder=e.kind==="guide"?`Describe alignment change at ${e.title}\u2026`:"What should change on this element\u2026";let p=document.createElement("div");if(p.className="pop-foot",e.guide){let d=document.createElement("button");d.type="button",d.className="pop-link",d.textContent="Remove guide",d.addEventListener("click",()=>{this.guides=this.guides.filter(h=>h.id!==e.guide?.id),this.closePopover(),this.renderPinsAndGuides()}),p.appendChild(d)}else{let d=document.createElement("span");d.className="pop-hint";let h=/mac/i.test(navigator.platform||navigator.userAgent);d.textContent=h?"\u2318Enter to add":"Ctrl+Enter to add",p.appendChild(d)}let c=document.createElement("div");c.className="pop-actions";let u=document.createElement("button");u.type="button",u.className="btn btn-sec",u.textContent="Cancel",u.addEventListener("click",()=>this.closePopover());let m=document.createElement("button");m.type="button",m.className="btn btn-pri",m.textContent="Add comment";let w=()=>{let d=l.value.trim();if(!d)return;let h=this.pins.length+1,f={id:`pin-${Date.now()}-${h}`,num:h,kind:e.kind,x:Math.round(e.x),y:Math.round(e.y),rect:e.rect,comment:d,element:e.summary?.element??e.title};this.pins.push(f),this.closePopover(),this.renderPinsAndGuides();let E={x:f.x,y:f.y,comment:d,element:e.summary?.element??e.title,elementPath:e.summary?.elementPath??e.guide?.targetSelector??e.title,fullPath:e.summary?.fullPath,cssClasses:e.summary?.cssClasses,nearbyText:e.summary?.nearbyText,computedStyles:e.summary?.computedStyles,reactComponents:e.summary?.reactComponents,boundingBox:e.rect??e.summary?.boundingBox,kind:e.kind};e.guide&&(E.measurement={axis:e.guide.axis,positionPx:e.guide.pos,toSelector:e.guide.targetSelector,deltaPx:e.guide.deltaPx}),this.postAnnotation(E)};m.addEventListener("click",w),l.addEventListener("keydown",d=>{(d.metaKey||d.ctrlKey)&&d.key==="Enter"&&(d.preventDefault(),w())}),c.append(u,m),p.appendChild(c),n.append(a,l,p),this.shadow.appendChild(n),this.popoverEl=n,l.focus(),requestAnimationFrame(()=>l.focus())}destroy(){this.setAnimationsPaused(!1),this.host.remove()}};function M(){let i=window;if(i.__TROUPE_ANNOTATE__)return i.__TROUPE_ANNOTATE_INSTANCE__;let e=new b;return i.__TROUPE_ANNOTATE_INSTANCE__=e,i.__TROUPE_ANNOTATE__={dispatch:n=>e.dispatch(n),unmount:()=>e.destroy()},i.TermulAgentation={mount:()=>e.dispatch({type:"set_active",active:!0}),unmount:()=>e.destroy()},e}function R(){document.readyState==="loading"?document.addEventListener("DOMContentLoaded",()=>{M()}):M()}R();return $(H);})();
