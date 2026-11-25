"use strict";var AgentationToolbar=(()=>{var y=Object.defineProperty;var S=Object.getOwnPropertyDescriptor;var k=Object.getOwnPropertyNames;var M=Object.prototype.hasOwnProperty;var T=(i,e)=>{for(var t in e)y(i,t,{get:e[t],enumerable:!0})},A=(i,e,t,n)=>{if(e&&typeof e=="object"||typeof e=="function")for(let o of k(e))!M.call(i,o)&&o!==t&&y(i,o,{get:()=>e[o],enumerable:!(n=S(e,o))||n.enumerable});return i};var $=i=>A(y({},"__esModule",{value:!0}),i);var I={};T(I,{buildElementPath:()=>m,buildShortLabel:()=>g});var O=["display","position","width","height","padding","margin","gap","font-size","line-height","font-weight","border-radius"];function g(i){let e=i.tagName.toLowerCase();if(i.id)return`${e}#${i.id}`;let t=typeof i.className=="string"?i.className.trim().split(/\s+/).filter(Boolean):[];return t.length>0?`${e}.${t.slice(0,2).join(".")}`:e}function m(i,e=4){let t=[],n=i;for(;n&&n!==document.body&&t.length<e&&(t.unshift(g(n)),!n.id);)n=n.parentElement;return t.join(" > ")||i.tagName.toLowerCase()}function B(i){let e=Object.keys(i).find(a=>a.startsWith("__reactFiber$")||a.startsWith("__reactInternalInstance$"));if(!e)return;let t=[],n=i[e],o=0;for(;n&&t.length<5&&o<40;){o+=1;let a=n.type,s=typeof a=="function"||typeof a=="object"&&a!==null?a.displayName||a.name:void 0;s&&/^[A-Z]/.test(s)&&!t.includes(s)&&t.push(s),n=n.return}return t.length>0?t.join(" < "):void 0}function L(i){let e=window.getComputedStyle(i),t=[];for(let n of O){let o=e.getPropertyValue(n);o&&o!=="normal"&&o!=="none"&&o!=="0px"&&o!=="static"&&t.push(`${n}: ${o}`)}return t.join("; ")}function P(i){let e=i.getBoundingClientRect(),t=(i.textContent??"").trim().replace(/\s+/g," ").slice(0,120),n=typeof i.className=="string"&&i.className.trim()?i.className.trim().slice(0,200):void 0;return{element:g(i),elementPath:m(i,4),fullPath:m(i,10),cssClasses:n,nearbyText:t||void 0,computedStyles:L(i)||void 0,reactComponents:B(i),boundingBox:{x:Math.round(e.x),y:Math.round(e.y),width:Math.round(e.width),height:Math.round(e.height)}}}function _(i,e,t){let n=document.querySelectorAll("main *, header *, section *, form *, article *"),o=null,a=25;for(let s of Array.from(n).slice(0,250)){if(s.id===t||s.closest(`#${t}`))continue;let d=s.getBoundingClientRect();if(d.width<16||d.height<12)continue;let l=i==="vertical"?d.left:d.top,c=Math.round(l-e),p=Math.abs(c);p>=1&&p<a&&(a=p,o={selector:g(s),elementPath:m(s,4),deltaPx:c,rect:{x:Math.round(d.x),y:Math.round(d.y),width:Math.round(d.width),height:Math.round(d.height)}})}return o}var v="termul-agentation-host",C="troupe-annotate-pause-animations",G=`
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
`,b=class{host;shadow;layer;hoverBox;dragBox;spacingPill;pinsContainer;guidesContainer;popoverEl=null;active=!1;tool="comment";pinsHidden=!1;pins=[];guides=[];sessionId=null;dragStart=null;constructor(){let e=document.getElementById(v);e&&e.remove(),this.host=document.createElement("div"),this.host.id=v,document.body.appendChild(this.host),this.shadow=this.host.attachShadow({mode:"open"});let t=document.createElement("style");t.textContent=G,this.shadow.appendChild(t),this.layer=document.createElement("div"),this.layer.className="layer",this.shadow.appendChild(this.layer),this.hoverBox=document.createElement("div"),this.hoverBox.className="hover-box",this.shadow.appendChild(this.hoverBox),this.dragBox=document.createElement("div"),this.dragBox.className="drag-box",this.shadow.appendChild(this.dragBox),this.spacingPill=document.createElement("div"),this.spacingPill.className="spacing-pill",this.shadow.appendChild(this.spacingPill),this.guidesContainer=document.createElement("div"),this.shadow.appendChild(this.guidesContainer),this.pinsContainer=document.createElement("div"),this.shadow.appendChild(this.pinsContainer),this.bindEvents()}getEndpoint(){return window.__TERMUL_AGENTATION_ENDPOINT__}async ensureSession(){if(this.sessionId)return this.sessionId;let e=window.__TERMUL_AGENTATION_SESSION_ID__;if(e)return this.sessionId=e,e;let t=this.getEndpoint();if(!t)return null;try{let n=await fetch(`${t}/sessions`,{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({url:window.location.href})});if(!n.ok)return null;let o=await n.json();if(o.id)return this.sessionId=o.id,o.id}catch{}return null}async postAnnotation(e){let t=this.getEndpoint();if(!t)return;let n=await this.ensureSession();if(n)try{await fetch(`${t}/sessions/${n}/annotations`,{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({url:window.location.href,timestamp:Date.now(),...e})})}catch{}}async postAction(e){let t=this.getEndpoint();if(!t)return;let n=await this.ensureSession();if(n)try{await fetch(`${t}/sessions/${n}/action`,{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({sessionId:n,annotations:[],output:JSON.stringify(e),timestamp:new Date().toISOString()})})}catch{}}dispatch(e){switch(e.type){case"set_active":this.active=!!e.active,this.active||(this.closePopover(),this.hoverBox.style.display="none",this.spacingPill.style.display="none"),this.syncLayerMode();break;case"set_tool":e.tool&&(this.tool=e.tool),this.closePopover(),this.hoverBox.style.display="none",this.spacingPill.style.display="none",this.syncLayerMode();break;case"set_pause":this.setAnimationsPaused(!!e.paused);break;case"set_pins_hidden":this.pinsHidden=!!e.hidden,this.renderPinsAndGuides();break;case"place_guide":if(e.axis){let t=e.axis==="vertical"?Math.round(window.innerWidth/3):64;this.addGuide(e.axis,t)}break;case"clear":this.pins=[],this.guides=[],this.closePopover(),this.renderPinsAndGuides();break}}setAnimationsPaused(e){let t=document.getElementById(C);if(e&&!t){let n=document.createElement("style");n.id=C,n.textContent="*, *::before, *::after { animation-play-state: paused !important; transition-duration: 0.001ms !important; }",document.head.appendChild(n)}else!e&&t&&t.remove()}syncLayerMode(){this.layer.className="layer",this.active&&(this.tool==="rectangle"||this.tool==="screenshot-area"?this.layer.classList.add("capture"):this.layer.classList.add("comment-mode"))}elementUnderPoint(e,t){this.host.style.display="none";let n=document.elementFromPoint(e,t);return this.host.style.display="",!n||n===document.documentElement||n===document.body?null:n}bindEvents(){this.layer.addEventListener("pointermove",e=>{if(!this.active||this.popoverEl)return;if(this.dragStart&&(this.tool==="rectangle"||this.tool==="screenshot-area")){let o=Math.min(this.dragStart.x,e.clientX),a=Math.min(this.dragStart.y,e.clientY),s=Math.abs(e.clientX-this.dragStart.x),d=Math.abs(e.clientY-this.dragStart.y);this.dragBox.style.display="block",this.dragBox.style.left=`${o}px`,this.dragBox.style.top=`${a}px`,this.dragBox.style.width=`${s}px`,this.dragBox.style.height=`${d}px`;return}let t=this.elementUnderPoint(e.clientX,e.clientY);if(!t){this.hoverBox.style.display="none",this.spacingPill.style.display="none";return}let n=t.getBoundingClientRect();this.hoverBox.style.display="block",this.hoverBox.style.left=`${n.left}px`,this.hoverBox.style.top=`${n.top}px`,this.hoverBox.style.width=`${n.width}px`,this.hoverBox.style.height=`${n.height}px`,this.tool==="ruler"?(this.spacingPill.style.display="block",this.spacingPill.textContent=`${Math.round(n.width)}\xD7${Math.round(n.height)}px \xB7 x ${Math.round(n.left)} y ${Math.round(n.top)}`,this.spacingPill.style.left=`${Math.min(window.innerWidth-180,n.left)}px`,this.spacingPill.style.top=`${Math.max(8,n.top-24)}px`):this.spacingPill.style.display="none"}),this.layer.addEventListener("pointerdown",e=>{if(!(!this.active||e.button!==0)){if(this.tool==="rectangle"||this.tool==="screenshot-area"){this.dragStart={x:e.clientX,y:e.clientY},this.dragBox.style.display="none";return}if(this.tool==="ruler"){let t=e.shiftKey?"horizontal":"vertical",n=Math.round(t==="vertical"?e.clientX:e.clientY),o=this.addGuide(t,n);this.openGuidePopover(o,e.clientX,e.clientY);return}if(this.tool==="comment"){let t=this.elementUnderPoint(e.clientX,e.clientY);if(!t)return;let n=P(t);this.openCommentPopover({title:n.element,x:e.clientX,y:e.clientY,kind:"feedback",summary:n})}}}),this.layer.addEventListener("pointerup",e=>{if(!this.dragStart)return;let t=Math.min(this.dragStart.x,e.clientX),n=Math.min(this.dragStart.y,e.clientY),o=Math.abs(e.clientX-this.dragStart.x),a=Math.abs(e.clientY-this.dragStart.y);if(this.dragStart=null,this.dragBox.style.display="none",o<8||a<8)return;let s={x:Math.round(t),y:Math.round(n),width:Math.round(o),height:Math.round(a)};if(this.tool==="screenshot-area"){this.postAction({kind:"screenshot_area",rect:s,dpr:window.devicePixelRatio||1});return}this.openCommentPopover({title:`Rectangle \xB7 ${s.width}\xD7${s.height}px`,x:s.x,y:s.y,kind:"feedback",rect:s})}),window.addEventListener("keydown",e=>{if(this.active){if(e.key==="Escape"){this.closePopover();return}this.popoverEl||(this.tool==="ruler"&&(e.key==="h"||e.key==="H")?this.addGuide("horizontal",Math.round(window.innerHeight/4)):this.tool==="ruler"&&(e.key==="v"||e.key==="V")&&this.addGuide("vertical",32))}})}addGuide(e,t){let n=_(e,t,v),o={id:`guide-${Date.now()}-${Math.random().toString(36).slice(2,6)}`,axis:e,pos:t,deltaPx:n?.deltaPx,targetSelector:n?.selector};return this.guides.push(o),this.renderPinsAndGuides(),o}renderPinsAndGuides(){if(this.pinsContainer.innerHTML="",this.guidesContainer.innerHTML="",!this.pinsHidden){for(let e of this.guides){let t=document.createElement("div");t.className=`guide ${e.axis}`,e.axis==="vertical"?t.style.left=`${e.pos}px`:t.style.top=`${e.pos}px`,t.addEventListener("click",s=>{s.stopPropagation(),this.openGuidePopover(e,s.clientX,s.clientY)}),this.guidesContainer.appendChild(t);let n=document.createElement("div");n.className="guide-badge";let o=e.axis==="vertical"?`x ${e.pos}px`:`y ${e.pos}px`,a=typeof e.deltaPx=="number"&&e.deltaPx!==0?` \xB7 ${e.deltaPx>0?`+${e.deltaPx}`:e.deltaPx}px off`:"";n.textContent=`${o}${a}`,e.axis==="vertical"?(n.style.left=`${Math.min(window.innerWidth-130,e.pos+6)}px`,n.style.bottom="14px"):(n.style.top=`${Math.min(window.innerHeight-28,e.pos+6)}px`,n.style.left="16px"),n.addEventListener("click",s=>{s.stopPropagation(),this.openGuidePopover(e,s.clientX,s.clientY)}),this.guidesContainer.appendChild(n)}for(let e of this.pins){if(e.rect){let n=document.createElement("div");n.className="saved-rect",n.style.left=`${e.rect.x}px`,n.style.top=`${e.rect.y}px`,n.style.width=`${e.rect.width}px`,n.style.height=`${e.rect.height}px`,this.pinsContainer.appendChild(n)}let t=document.createElement("div");t.className=e.kind==="guide"?"pin guide-pin":"pin",t.textContent=String(e.num),t.title=`${e.element}: ${e.comment}`,t.style.left=`${e.x}px`,t.style.top=`${e.y}px`,this.pinsContainer.appendChild(t)}}}closePopover(){this.popoverEl&&(this.popoverEl.remove(),this.popoverEl=null)}openGuidePopover(e,t,n){let o=`${e.axis==="vertical"?"Vertical":"Horizontal"} guide \xB7 ${e.axis==="vertical"?"x":"y"} = ${e.pos}px`;this.openCommentPopover({title:o,x:e.axis==="vertical"?e.pos:t,y:e.axis==="horizontal"?e.pos:n,kind:"guide",guide:e})}openCommentPopover(e){this.closePopover();let t=document.createElement("div");t.className="popover";let n=Math.max(12,Math.min(window.innerWidth-348,e.x+14)),o=Math.max(12,Math.min(window.innerHeight-190,e.y+14));t.style.left=`${n}px`,t.style.top=`${o}px`;let a=document.createElement("div");a.className="pop-head";let s=document.createElement("span");s.className="pop-title",s.textContent=e.title;let d=document.createElement("span");d.className="pop-pill",d.textContent="Crop attached",a.append(s,d);let l=document.createElement("textarea");l.className="pop-input",l.placeholder=e.kind==="guide"?`Describe alignment change at ${e.title}\u2026`:"What should change on this element\u2026";let c=document.createElement("div");if(c.className="pop-foot",e.guide){let r=document.createElement("button");r.type="button",r.className="pop-link",r.textContent="Remove guide",r.addEventListener("click",()=>{this.guides=this.guides.filter(x=>x.id!==e.guide?.id),this.closePopover(),this.renderPinsAndGuides()}),c.appendChild(r)}else{let r=document.createElement("span");r.className="pop-hint",r.textContent="\u2318Enter to add",c.appendChild(r)}let p=document.createElement("div");p.className="pop-actions";let h=document.createElement("button");h.type="button",h.className="btn btn-sec",h.textContent="Cancel",h.addEventListener("click",()=>this.closePopover());let u=document.createElement("button");u.type="button",u.className="btn btn-pri",u.textContent="Add comment";let w=()=>{let r=l.value.trim();if(!r)return;let x=this.pins.length+1,f={id:`pin-${Date.now()}-${x}`,num:x,kind:e.kind,x:Math.round(e.x),y:Math.round(e.y),rect:e.rect,comment:r,element:e.summary?.element??e.title};this.pins.push(f),this.closePopover(),this.renderPinsAndGuides();let E={x:f.x,y:f.y,comment:r,element:e.summary?.element??e.title,elementPath:e.summary?.elementPath??e.guide?.targetSelector??e.title,fullPath:e.summary?.fullPath,cssClasses:e.summary?.cssClasses,nearbyText:e.summary?.nearbyText,computedStyles:e.summary?.computedStyles,reactComponents:e.summary?.reactComponents,boundingBox:e.rect??e.summary?.boundingBox,kind:e.kind};e.guide&&(E.measurement={axis:e.guide.axis,positionPx:e.guide.pos,toSelector:e.guide.targetSelector,deltaPx:e.guide.deltaPx}),this.postAnnotation(E)};u.addEventListener("click",w),l.addEventListener("keydown",r=>{(r.metaKey||r.ctrlKey)&&r.key==="Enter"&&(r.preventDefault(),w())}),p.append(h,u),c.appendChild(p),t.append(a,l,c),this.shadow.appendChild(t),this.popoverEl=t,l.focus()}destroy(){this.setAnimationsPaused(!1),this.host.remove()}};function N(){let i=window;if(i.__TROUPE_ANNOTATE__)return i.__TROUPE_ANNOTATE_INSTANCE__;let e=new b;return i.__TROUPE_ANNOTATE_INSTANCE__=e,i.__TROUPE_ANNOTATE__={dispatch:t=>e.dispatch(t),unmount:()=>e.destroy()},i.TermulAgentation={mount:()=>e.dispatch({type:"set_active",active:!0}),unmount:()=>e.destroy()},e}function R(){document.readyState==="loading"?document.addEventListener("DOMContentLoaded",()=>{N()}):N()}R();return $(I);})();
