(function(){let e=document.createElement(`link`).relList;if(e&&e.supports&&e.supports(`modulepreload`))return;for(let e of document.querySelectorAll(`link[rel="modulepreload"]`))n(e);new MutationObserver(e=>{for(let t of e)if(t.type===`childList`)for(let e of t.addedNodes)e.tagName===`LINK`&&e.rel===`modulepreload`&&n(e)}).observe(document,{childList:!0,subtree:!0});function t(e){let t={};return e.integrity&&(t.integrity=e.integrity),e.referrerPolicy&&(t.referrerPolicy=e.referrerPolicy),t.credentials=e.crossOrigin===`use-credentials`?`include`:e.crossOrigin===`anonymous`?`omit`:`same-origin`,t}function n(e){if(e.ep)return;e.ep=!0;let n=t(e);fetch(e.href,n)}})();var e=Object.freeze({development:!1,bindingWarningThreshold:50});function t(){return e}function n(){return e.development}function r(e,t){if(typeof e!=`string`||e.length===0)return``;let n=e.toLowerCase(),r,a=1/0;for(let e of t){let t=i(n,e.toLowerCase());t<a&&(r=e,a=t)}let o=Math.max(1,Math.floor(n.length/3));return r&&a<=o?` Did you mean "${r}"?`:``}function i(e,t){let n=Array.from({length:t.length+1},(e,t)=>t);for(let r=1;r<=e.length;r+=1){let i=n[0];n[0]=r;for(let a=1;a<=t.length;a+=1){let o=n[a];n[a]=e[r-1]===t[a-1]?i:Math.min(i,n[a-1],o)+1,i=o}}return n[t.length]}var a=new Map([[`renderer`,new Set],[`scheduler`,new Set],[`logger`,new Set],[`style`,new Set]]);function o(e){let t=a.get(e);if(!t)throw TypeError(`Unknown plugin extension point: ${e}${r(e,[...a.keys()])}`);return t}function s(e,t){let n=o(e);if(n.size!==0)for(let e of[...n])e(t)}var c=[`/src/utils/diagnostics.js`,`/src/components/index.js`,`/src/reactivity/context.js`,`/src/reactivity/source.js`,`/src/reactivity/signal.js`,`/src/reactivity/computed.js`,`/src/dom/template.js`,`/src/utils/form.js`];function l(e){if(e===null)return`null`;if(e===void 0)return`undefined`;if(typeof e==`function`)return e.name?`function ${e.name}`:`an anonymous function`;if(typeof e==`object`)try{let t=e.constructor?.name;return t?`an object (${t})`:`an object`}catch{return`an object`}try{return`${typeof e} (${String(e)})`}catch{return typeof e}}function u(e,t={}){try{let n=`[Matrix] ${e}`;t.stack?globalThis.console?.warn?.(`${n}\n${t.stack}`):globalThis.console?.warn?.(n)}catch{}try{s(`logger`,{...t,type:t.type??`warning`,message:e})}catch{}}function d(){let e=Error().stack;return e?e.split(`
`).slice(2).filter(e=>!c.some(t=>e.includes(t))).join(`
`):``}function f(e,t={}){n()&&u(e,t)}var p=null,m=null,h=null,g=[],_=[],v=[],y=new WeakSet;function b(e,t){g.push(p),p=e;try{return t()}finally{p=g.pop()}}function x(e){if(!p){n()&&!h&&!y.has(e)&&(y.add(e),f(`${e.kind} "${e.name||`anonymous`}" was read outside an Effect or template. Use .peek() for an intentional non-reactive read.`,{type:`reactivity:untracked-read`,kind:e.kind,name:e.name,source:e,stack:d()}));return}p!==e&&(e.subscribers.add(p),p.dependencies.add(e))}function S(){return m}function C(e,t){_.push(m),m=e;try{return t()}finally{m=_.pop()}}function w(){return h}function T(e,t){v.push(h),h=e;try{return t()}finally{h=v.pop()}}var ee=1,E=new Set;function D(e,t=``){let n={id:`source-${ee++}`,kind:e,name:t,subscribers:new Set,listeners:new Set};return E.add(n),n}function te(e){E.delete(e)}function ne(e){x(e)}function re(e,t,n){let r;for(let t of[...e.subscribers])try{t._notify()}catch(e){r??=e}for(let i of[...e.listeners])try{i(t,n)}catch(e){r??=e}if(r)throw r}function ie(e,t){if(typeof t!=`function`)throw TypeError(`signal.subscribe() expects a function`);return e.listeners.add(t),()=>{e.listeners.delete(t)}}function O(e,t={}){let n=w();if(n?.isRendering){let r=n.stateCursor;n.stateCursor+=1;let i=n.stateSlots[r];if(i){if(i.kind!==`signal`)throw Error(`Component state order changed at slot ${r}`);return i.value}let a=n.stateScope?n.stateScope.run(()=>k(e,t)):k(e,t);return n.stateSlots[r]={kind:`signal`,value:a},a}return k(e,t)}function k(e,t){let n=t.equals??Object.is,r=D(`signal`,t.name??``),i=e,a=!1,o={get value(){if(a)throw Error(`Cannot read a disposed signal`);return ne(r),i},set value(e){if(a)throw Error(`Cannot write to a disposed signal`);if(n(i,e))return;let t=i;i=e,re(r,i,t)},get(){return o.value},set(e){return o.value=e,i},update(e){if(typeof e!=`function`)throw TypeError(`signal.update() expects a function`);return o.value=e(i),i},peek(){if(a)throw Error(`Cannot read a disposed signal`);return i},dispose(){a||(a=!0,r.subscribers.clear(),r.listeners.clear(),te(r))},get name(){return t.name??``},subscribe(e){return ie(r,e)},get kind(){return r.kind},_source:r};r.read=o.peek;let s=S();return s&&s.add(o.dispose),o}var A=0,ae=!1,oe=!1,se=new Set;function ce(){return A>0}function le(){if(oe)return;oe=!0;let e=()=>{oe=!1,de()};typeof queueMicrotask==`function`?queueMicrotask(e):Promise.resolve().then(e)}function ue(e,t=`sync`){if(s(`scheduler`,{type:`job:scheduled`,flush:t}),t===`microtask`||A>0){se.add(e),(A===0||t===`microtask`)&&le();return}e()}function de(){if(ae||A>0)return;ae=!0,s(`scheduler`,{type:`flush:start`,size:se.size});let e;try{for(;se.size>0;){let t=[...se];se.clear();for(let n of t)try{n()}catch(t){e??=t}}}finally{ae=!1,s(`scheduler`,{type:`flush:end`})}if(e)throw e}var fe=100,pe=new Set,me=1;function j(e,t={}){if(typeof e!=`function`)throw TypeError(`effect() expects a function`);let n=t.flush??`sync`;if(n!==`sync`&&n!==`microtask`)throw TypeError(`effect() accepts flush: 'sync' or 'microtask'. Received ${l(n)}${r(n,[`sync`,`microtask`])}`);let i=new Set,a=typeof t.onError==`function`?t.onError:null,o=t.warnOnDependencyChange??!0,s=new Set,c=!1,d=!1,f=!1,p,m=!1,h=!1,g=!1,_=!1,v={id:`effect-${me++}`,kind:`effect`,name:t.name??``,dependencies:i,_notify(){if(!g){if(m){h=!0;return}w()}}};function y(){for(let e of i)e.subscribers.delete(v);i.clear()}function x(){if(typeof p!=`function`)return;let e=p;p=void 0,e()}function C(){if(g||m)return;m=!0;let t=0;try{do{if(h=!1,t+=1,t>fe)throw Error(`Reactive loop detected in effect()`);let n=typeof p==`function`;x(),y();let r=b(v,e),a=!!(r&&typeof r.then==`function`);if(a&&!f&&(f=!0,u(`Effect "${v.name||`anonymous`}" returned a Promise. Matrix does not await Effects; cancel async work in cleanup or use resource() to avoid stale closures.`,{type:`effect:async-return`,name:v.name,source:v})),c){let e=[...i].filter(e=>!s.has(e)),t=[...s].filter(e=>!i.has(e));(e.length>0||t.length>0)&&o&&u(`Effect "${v.name||`anonymous`}" changed dependencies (${e.length} added, ${t.length} removed). Return cleanup to cancel work from the previous run and prevent stale closures.`,{type:`effect:dependencies-changed`,name:v.name,added:e.length,removed:t.length,staleClosureRisk:d||!n,source:v})}s=new Set(i),d=a,c=!0,typeof r==`function`&&(p=r)}while(h&&!g)}catch(e){try{T()}catch{}if(a){a(e);return}throw e}finally{m=!1}}function w(){_||g||(_=!0,ue(()=>{_=!1,C()},n))}function T(){if(g)return;g=!0,_=!1;let e;try{x()}catch(t){e=t}try{y()}catch(t){e??=t}if(E&&E(),pe.delete(v),e)throw e}let ee=S(),E=ee?ee.add(T):null;try{pe.add(v),C()}catch(e){throw T(),e}return T}function M(e,t={}){let n=w();if(n?.isRendering){let r=n.stateCursor;n.stateCursor+=1;let i=n.stateSlots[r];if(i){if(i.kind!==`computed`)throw Error(`Component state order changed at slot ${r}`);return i.value}let a=n.stateScope?n.stateScope.run(()=>he(e,t)):he(e,t);return n.stateSlots[r]={kind:`computed`,value:a},a}return he(e,t)}function he(e,t){let n=typeof e==`function`?e:e?.get,r=e&&typeof e==`object`?e.set:void 0;if(typeof n!=`function`)throw TypeError(`computed() expects a function`);if(r!==void 0&&typeof r!=`function`)throw TypeError(`computed() expects a valid setter`);let i=t.equals??Object.is,a=D(`computed`,t.name??``),o=new Set,s,c=!0,l=!1,u=!1,d=!1;function f(){if(d=!1,u||!c||a.subscribers.size===0&&a.listeners.size===0)return;let e=s;h(),i(e,s)||re(a,s,e)}let p={id:`computed-${a.id}`,kind:`computed`,dependencies:o,_notify(){if(!(u||c)&&(c=!0,a.subscribers.size!==0||a.listeners.size!==0)){if(ce()){d||(d=!0,ue(f));return}f()}}};function m(){for(let e of o)e.subscribers.delete(p);o.clear()}function h(){if(!c||u)return s;if(l)throw Error(`Reactive loop detected in computed()`);l=!0,m();try{s=b(p,n),c=!1}finally{l=!1}return s}let g={get value(){if(u)throw Error(`Cannot read a disposed computed value`);return ne(a),h()},get(){return g.value},set value(e){if(!r)throw TypeError(`This computed value is read-only`);r(e)},set(e){return g.value=e,g.value},peek(){return h()},subscribe(e){return ie(a,e)},get kind(){return a.kind},get name(){return t.name??``},_source:a};function _(){u||(u=!0,m(),a.subscribers.clear(),a.listeners.clear(),te(a))}g.dispose=_,a.read=g.peek;let v=S();return v&&v.add(_),g}function ge(e){typeof e==`function`&&e()}function N(e=S()){let t=new Set,n=new Set,r=!1,i={get disposed(){return r},run(e){if(r)throw Error(`Cannot use a disposed scope`);return C(i,e)},add(e){if(typeof e!=`function`)throw TypeError(`A cleanup must be a function`);return r?(ge(e),()=>{}):(n.add(e),()=>{n.delete(e)})},dispose(){if(r)return;r=!0;let a;for(let e of[...t])try{e.dispose()}catch(e){a??=e}t.clear();for(let e of[...n]){n.delete(e);try{ge(e)}catch(e){a??=e}}if(e&&e._children.delete(i),a)throw a},_children:t};return e&&e._children.add(i),i}var _e=null,ve=[],ye=Symbol(`matrix.error.boundary`);function be(e,t){ve.push(_e),_e=e;try{return t()}finally{_e=ve.pop()}}function xe(){return _e}var Se=Symbol(`matrix.component.result`);function Ce(e,t,n){let r=String(n);f(`Component "${e||`anonymous`}" props are read-only. Cannot ${t} "${r}". Update the owner state instead.`,{type:`component:prop-mutation`,name:e||`anonymous`,operation:t,property:r,stack:d()})}function we(e,t){return new Proxy(e,{set(e,n){throw Ce(t,`set`,n),TypeError(`Component props are read-only`)},deleteProperty(e,n){throw Ce(t,`delete`,n),TypeError(`Component props are read-only`)}})}function Te(e,t={},n){if(typeof e!=`function`)throw TypeError(`component() expects a render function. Received ${l(e)}. Pass a function such as component(props => html\`<div>...</div>\`).`);let r=we(t&&typeof t==`object`?t:{},e.name),i={[Se]:!0,key:n,render:e,props:r,update(t){return t?.render===e&&t?.key===n}};return Object.defineProperty(i,"_matrixSourceLocation",{value:d(),enumerable:!1}),i}function Ee(e){return!!(e&&e[Se])}function De(e){if(typeof e!=`function`)throw TypeError(`onMount() expects a function`);let t=xe();if(!t)throw Error(`onMount() must be called inside a component`);t.isMounted||t.mountCallbacks.push(e)}var Oe=new WeakMap,ke=/__MATRIX_ATTR_(\d+)__/g,Ae=/^matrix:text:(\d+)$/,je=/(?:^|[>\s])\{\s*([A-Za-z_$][\w$]*(?:\s*\.\s*[A-Za-z_$][\w$]*)*)\s*\}(?=\s|<|$)/,Me=/\\\$\{\s*([^}]+)\}/,Ne=Symbol(`matrix.template.result`);function P(e,...t){if(!Array.isArray(e)||!Array.isArray(e.raw))throw TypeError(`html() must be used as a tagged template`);return Pe(e),{[Ne]:!0,strings:e,values:t}}function Pe(e){let t=e.raw.join(``),n=Me.exec(t);if(n){let e=n[1].trim(),t="${"+e+`}`;f(`Template contains an escaped interpolation "${"\\${"+e+`}`}". Did you mean "${t}"?`,{type:`template:forgotten-interpolation`,expression:e,stack:d()});return}let r=je.exec(t);if(!r)return;let i=r[1].replace(/\s*\.\s*/g,`.`);f(`Template contains "{${i}}". Did you mean "${"${"+i+`}`}"?`,{type:`template:forgotten-interpolation`,expression:i,stack:d()})}function Fe(e){return!!(e&&e[Ne])}function Ie(e,t){let n=e+t,r=n.lastIndexOf(`<`);if(r<=n.lastIndexOf(`>`))return!1;let i=n.slice(r);return/(?:^|\s)([^\s="'<>`]+)\s*=\s*(?:"[^"]*|'[^']*|[^\s"'<>`]*)$/.test(i)}function Le(e){return e.reduce((t,n,r)=>{if(r===e.length-1)return t+n;let i=Ie(t,n);if(!i&&/<\/?[A-Za-z0-9_-]*$/.test(n))throw Error(`Expressions cannot be used inside a tag name`);let a=i?`__MATRIX_ATTR_${r}__`:`<!--matrix:text:${r}-->`;return t+n+a},``)}function Re(e){let t=[],n=e.ownerDocument.createTreeWalker(e,128),r=n.nextNode();for(;r;){let i=Ae.exec(r.data);i&&t.push({path:Be(r,e),index:Number(i[1])}),r=n.nextNode()}return t}function ze(e){let t=[],n=e.querySelectorAll(`*`);for(let r of n)for(let n of[...r.attributes]){let i=[...n.value.matchAll(ke)];if(i.length===0)continue;let a=[],o=0;for(let e of i)e.index>o&&a.push(n.value.slice(o,e.index)),a.push({index:Number(e[1])}),o=e.index+e[0].length;o<n.value.length&&a.push(n.value.slice(o)),t.push({path:Be(r,e),name:n.name,parts:a})}return t}function Be(e,t){let n=[],r=e;for(;r!==t;){let e=r.parentNode;if(!e)throw Error(`Matrix could not index a compiled template node`);n.unshift([...e.childNodes].indexOf(r)),r=e}return n}function Ve(e,t){let n=e;for(let e of t)n=n.childNodes[e];return n}function He(e,t){let n=Oe.get(e);n||(n=new WeakMap,Oe.set(e,n));let r=n.get(t);if(r)return r;let i=t.createElement(`template`);return i.innerHTML=Le(e),r={template:i,textBindings:Re(i.content),attributeBindings:ze(i.content)},n.set(t,r),r}function Ue(e,t){return e.map(e=>typeof e==`string`?e:t[e.index])}function We(e){return!(!e||typeof e!=`object`||e.kind!==`signal`&&e.kind!==`computed`||typeof e.get!=`function`)}var Ge=new WeakMap,Ke=new WeakMap,qe=Symbol(`matrix.style.result`),Je=Symbol(`matrix.variables.result`),Ye=/[<>{};\u0000-\u001f\u007f]|(?:expression|behavior)\s*\(|(?:^|[^A-Za-z0-9_-])(?:javascript|vbscript|data):|url\(\s*["']?\s*(?:javascript|vbscript|data):/i;Object.freeze({"--matrix-color-primary":`#2563eb`,"--matrix-color-surface":`#ffffff`,"--matrix-color-text":`#0f172a`,"--matrix-space-1":`0.25rem`,"--matrix-space-2":`0.5rem`,"--matrix-space-3":`0.75rem`,"--matrix-radius-sm":`0.375rem`,"--matrix-radius-md":`0.5rem`,"--matrix-font-body":`system-ui, sans-serif`});function Xe(e){let t=2166136261;for(let n=0;n<e.length;n+=1)t^=e.charCodeAt(n),t=Math.imul(t,16777619);return Math.abs(t>>>0).toString(36)}function Ze(e,t){return e.reduce((n,r,i)=>i===e.length-1?n+r:n+r+(t[i]??``),``)}function Qe(e){return{[qe]:!0,id:`matrix-global-${Xe(e)}`,scopeSelector:null,cssText:e}}function $e(e){let t=Ge.get(e);return t||(t=new Map,Ge.set(e,t)),t}function et(e,t){let n=$e(e);if(n.has(t.id))return n.get(t.id);let r=e.createElement(`style`);return r.setAttribute(`data-matrix-style`,t.id),r.textContent=t.cssText,e.head?.appendChild(r),r.parentNode||e.documentElement.appendChild(r),n.set(t.id,r),r}function tt(e,...t){let n=typeof e==`string`?e:Ze(e,t);if(typeof e!=`string`&&t.length===0){let t=Ke.get(e);if(t)return t;let r=Qe(n);return Ke.set(e,r),r}return Qe(n)}function nt(e,t){let n=We(t)?t.value:t;if(n==null||n===!1)return null;if(typeof n==`object`||typeof n==`function`||typeof n==`symbol`)throw TypeError(`CSS custom property "${e}" expects a primitive value or reactive value`);let r=String(n);if(Ye.test(r))throw Error(`Unsafe CSS custom property value rejected for ${e}`);return r}function rt(e){return!!(e&&e[qe])}function it(e){return!!(e&&e[Je])}function at(e,t,n){if(!rt(t))throw TypeError(`use:style expects a css() result`);et(e.ownerDocument,t),t.scopeSelector&&e.setAttribute(`data-matrix-scope`,t.id),s(`style`,{type:`style:apply`,element:e,definition:t}),n.add(()=>{t.scopeSelector&&e.removeAttribute(`data-matrix-scope`)})}function ot(e,t,n){if(!it(t))throw TypeError(`use:vars expects a cssVariables() result`);let r=new Set;j(()=>{let n=new Set(Object.keys(t.values));for(let t of r)n.has(t)||e.style.removeProperty(t);for(let[n,r]of Object.entries(t.values)){let t=nt(n,r);t===null?e.style.removeProperty(n):e.style.setProperty(n,t)}r.clear();for(let e of n)r.add(e)}),n.add(()=>{for(let t of r)e.style.removeProperty(t)})}function st(e){return e.type===`checkbox`?e.checked:e.type===`radio`?e.checked?e.value:void 0:e.type===`file`?e.files:e.type===`number`||e.type===`range`?e.value===``?``:e.valueAsNumber:e.multiple&&e.options?[...e.selectedOptions].map(e=>e.value):e.value}function ct(e,t){if(e.type===`checkbox`){e.checked=!!t;return}if(e.type===`radio`){e.checked=String(t??``)===e.value;return}if(e.type===`file`)return;if(e.multiple&&e.options&&Array.isArray(t)){let n=new Set(t.map(String));for(let t of e.options)t.selected=n.has(t.value);return}let n=t??``;e.value!==String(n)&&(e.value=n)}function lt(e,t,n){let r=t,i=r?.source??r,a=Number(r?.debounce??0),o=r?.sanitize;if(o!==void 0&&typeof o!=`function`)throw TypeError(`use:bind sanitize expects a function`);if(!We(i)||i.kind!==`signal`||typeof i.set!=`function`)throw TypeError(`use:bind expects a writable signal`);let s=!1,c,l=!1;j(()=>{let t=i.value;l||ct(e,t)});let u=()=>{c=void 0;let t=st(e);if(t!==void 0){l=!0;try{i.value=o?o(t):t}finally{l=!1}}},d=()=>{if(!s){if(a>0){clearTimeout(c),c=setTimeout(u,a);return}u()}},f=()=>{s=!0},p=()=>{s=!1,d()};e.addEventListener(`input`,d),e.addEventListener(`change`,d),e.addEventListener(`compositionstart`,f),e.addEventListener(`compositionend`,p),n.add(()=>{clearTimeout(c),e.removeEventListener(`input`,d),e.removeEventListener(`change`,d),e.removeEventListener(`compositionstart`,f),e.removeEventListener(`compositionend`,p)})}var ut=1,dt=new Map;function ft(e){e.type?.startsWith(`dom:`)&&s(`renderer`,e),s(`logger`,e)}function pt(e){let t=`component-${ut++}`;return e.devtoolsId=t,dt.set(t,e),t}function mt(e){e?.devtoolsId&&dt.delete(e.devtoolsId)}var ht=Symbol(`matrix.keyed.list`);function gt(e){return!!(e&&e[ht])}var _t=`@`,vt=`.`,yt=`?`,bt=new Set([`href`,`src`,`action`,`formaction`,`poster`,`xlink:href`]),xt=new WeakSet;function St(e){try{ft(e)}catch{}}function F(e){return!(!e||typeof e!=`object`||e.kind!==`signal`&&e.kind!==`computed`||typeof e.get!=`function`)}function Ct(e){return!!(e&&typeof e.nodeType==`number`&&typeof e.nodeName==`string`)}function wt(e){return e==null||typeof e==`boolean`||F(e)||Fe(e)||gt(e)||Ee(e)||Array.isArray(e)||Ct(e)||typeof e==`function`}function Tt(e,t,n){if(wt(n))return;let r=l(n);if(e.invalidOutputWarnings.has(r))return;e.invalidOutputWarnings.add(r);let i=t.render.name||`anonymous`;u(`Component "${i}" returned ${r}. Return html\`...\`, a component, a Signal or Computed, an array, a DOM node, or null. The value will render as text.`,{type:`component:invalid-output`,name:i,valueType:typeof n,source:t.render})}function Et(e,n,r){let i=n.length+r.length,{development:a,bindingWarningThreshold:o}=t();!a||i<=o||xt.has(e.strings)||(xt.add(e.strings),f(`Template has ${i} dynamic bindings. Split large views into components or move derived work into Computeds to keep updates local.`,{type:`performance:unoptimized-bindings`,bindingCount:i,textBindings:n.length,attributeBindings:r.length}))}function Dt(e,t){let n=t.render.name;if(!n)return e;let r=`[${n}]`,i=e instanceof Error?e:Error(`${r} ${String(e)}`,{cause:e});i.message.startsWith(r)||(i.message=`${r} ${i.message}`);let a=t._matrixSourceLocation;if(i.stack){let e=i.stack.split(`
`);e[0]=`${i.name}: ${i.message}`,a&&!i.stack.includes(`Component "${n}" was created here`)&&(e.push(`\n[Matrix] Component "${n}" was created here:`),e.push(a)),i.stack=e.join(`
`)}return i}function Ot(e){return F(e)?e.value:e}function kt(e){e.parentNode&&e.parentNode.removeChild(e)}function At(e,t){let n=e;for(;n;){let e=n.nextSibling;if(kt(n),n===t)break;n=e}}function I(e){let t;for(let n of[...e].reverse())try{n?.dispose?.()}catch(e){t??=e}if(t)throw t}function L(e){try{e?.dispose?.()}catch{}}function jt(e,t){if(!bt.has(e.toLowerCase())||typeof t!=`string`)return;let n=t.indexOf(`:`),r=n===-1?``:t.slice(0,n).replace(/[\u0000-\u0020\u007f]+/g,``).toLowerCase();if(r===`javascript`||r===`vbscript`||r===`data`)throw Error(`Unsafe dynamic URL rejected for attribute ${e}`)}function Mt(){return{firstNode:null,get nodes(){return[]},dispose(){},moveBefore(){}}}function Nt(e,t,n){let r=t.ownerDocument.createTextNode(String(e));return t.insertBefore(r,n),{nodes:[r],firstNode:r,moveBefore(e){t.insertBefore(r,e)},dispose(){kt(r)}}}function Pt(e,t,n){return t.insertBefore(e,n),{nodes:[e],firstNode:e,moveBefore(n){t.insertBefore(e,n)},dispose(){kt(e)}}}function Ft(e,t,n,r){let i=[];try{for(let a of e)i.push(Lt(a,t,n,r))}catch(e){throw L({dispose:()=>I(i)}),e}let a=!1;return{get firstNode(){return i.find(e=>e.firstNode)?.firstNode??null},get nodes(){return i.flatMap(e=>e.nodes)},moveBefore(e){for(let t of i)t.moveBefore(e)},dispose(){a||(a=!0,I(i))}}}function It(e){return e===!0||e===!1?String(e):e}function Lt(e,t,n,r){let i=N(r),a=xe(),o=Mt(),s=!1,c=e=>a?be(a,()=>R(e,t,n,i)):R(e,t,n,i),l={get firstNode(){return o.firstNode},get nodes(){return o.nodes},moveBefore(e){o.moveBefore(e)},dispose(){if(s)return;s=!0;let e;try{o.dispose()}catch(t){e=t}try{i.dispose()}catch(t){e??=t}if(e)throw e}},u,d=!1;function f(e){if(d&&Object.is(u,e))return;if(typeof o.canUpdate==`function`&&o.canUpdate(e)){o.update(e),u=e,d=!0;return}let t=b(null,()=>c(e));try{o.dispose()}catch(e){throw L(t),e}o=t,u=e,d=!0}try{i.run(()=>{F(e)?j(()=>{f(It(e.value)),ft({type:`dom:update`,kind:`content`,parent:t,source:e})},{name:`render-dynamic-value`,warnOnDependencyChange:!1}):f(It(e))})}catch(e){throw L(l),e}return l}function R(e,t,n,r){return e==null||e===!1||e===!0?Mt():F(e)?Lt(e,t,n,r):Fe(e)?Gt(e,t,n,r):gt(e)?Kt(e,t,n,r):Ee(e)?Rt(e,t,n,r):typeof e==`function`?Rt(Te(e),t,n,r):Array.isArray(e)?Ft(e,t,n,r):Ct(e)?Pt(e,t,n):Nt(e,t,n)}function Rt(e,t,n,r){let i=N(r),a=xe(),o={scope:i,mountCallbacks:[],parent:a,provides:new Map,isErrorBoundary:!!e[ye],result:e,stateScope:i,stateSlots:[],stateCursor:0,isRendering:!1,isMounted:!1,invalidOutputWarnings:new Set};pt(o);let s,c,l=N(i);try{l.run(()=>{s=zt(o,e),c=be(o,()=>R(s,t,n,l))});let r=c.nodes.find(e=>e.nodeType===1)??c.nodes[0]??null;for(let e of o.mountCallbacks){let t=i.run(()=>e(r));typeof t==`function`&&i.add(t)}o.mountCallbacks.length=0,o.isMounted=!0,St({type:`component:mount`,id:o.devtoolsId,name:e.render.name||`anonymous`,parentId:a?.devtoolsId??null})}catch(s){L(c),L(l),L(i),St({type:`component:error`,id:o.devtoolsId,name:e.render.name||`anonymous`,message:s?.message??String(s)}),mt(o);let u=Dt(s,e),d=a;for(;d;){if(d.isErrorBoundary&&!d.handling){d.handling=!0;try{return R(typeof d.result.fallback==`function`?d.result.fallback(u):d.result.fallback,t,n,r)}finally{d.handling=!1}}d=d.parent}throw u}return{get firstNode(){return c.firstNode},get nodes(){return c.nodes},moveBefore(e){c.moveBefore(e)},canUpdate(t){return e.update?.(t)===!0},update(r){if(!this.canUpdate(r))return!1;let a=s,u=c,d=l,f=e,p=o.stateSlots.slice();o.mountCallbacks.length=0;let m=N(i),h,g;try{m.run(()=>{h=zt(o,r),g=be(o,()=>R(h,t,n,m))}),u.dispose(),d.dispose(),s=h,c=g,l=m,e=r,o.result=r,St({type:`component:update`,id:o.devtoolsId,name:r.render.name||`anonymous`,parentId:o.parent?.devtoolsId??null})}catch(t){L(g),L(m);for(let e=p.length;e<o.stateSlots.length;e+=1)L(o.stateSlots[e]?.value);o.stateSlots.length=p.length;for(let e=0;e<p.length;e+=1)o.stateSlots[e]=p[e];throw o.mountCallbacks.length=0,s=a,c=u,l=d,e=f,o.result=f,Dt(t,r)}return o.mountCallbacks.length=0,!0},dispose(){if(o.disposed)return;o.disposed=!0;let e;try{c.dispose()}catch(t){e=t}try{l.dispose()}catch(t){e??=t}try{i.dispose()}catch(t){e??=t}mt(o);try{St({type:`component:unmount`,id:o.devtoolsId,name:o.result.render.name||`anonymous`})}catch(t){e??=t}if(e)throw e}}}function zt(e,t){let n=e.stateSlots.length;e.stateCursor=0,e.isRendering=!0;try{let r=be(e,()=>T(e,()=>t.render(t.props)));if(e.isMounted&&e.stateCursor!==n)throw Error(`Component state order changed: expected ${n} slots, received ${e.stateCursor}`);return Tt(e,t,r),r}finally{e.isRendering=!1}}function Bt(e,t){let n=Ue(e,t).map(Ot);return n.length===1&&typeof e[0]!=`string`?n[0]:n.join(``)}function Vt(e,t){let n=Ue(e,t);return n.length===1&&typeof e[0]!=`string`?n[0]:Bt(e,t)}function Ht(e,t,n){if(t==null||t===!1)return e.removeAttribute(`style`),new Set;if(typeof t==`string`)return e.style.cssText=t,new Set;if(typeof t!=`object`)return e.style.cssText=String(t),new Set;let r=new Set(Object.keys(t));for(let t of n)r.has(t)||e.style.removeProperty(t);for(let[n,r]of Object.entries(t))r==null||r===!1?e.style.removeProperty(n):e.style.setProperty(n,r);return r}function Ut(e,t,n,r,i){let[a,...o]=t.split(`.`),s={once:o.includes(`once`),capture:o.includes(`capture`),passive:o.includes(`passive`)},c;j(()=>{let t=Bt(n,r);c&&e.removeEventListener(a,c,s),typeof t==`function`?(c=e=>{o.includes(`prevent`)&&e.preventDefault(),o.includes(`stop`)&&e.stopPropagation(),t(e)},e.addEventListener(a,c,s)):c=void 0},{flush:`sync`}),i.add(()=>{c&&e.removeEventListener(a,c,s)})}function Wt(e,t,n,r){let{name:i,parts:a}=t,o=new Set;if(i===`use:style`){at(e,Vt(a,n),r),e.removeAttribute(i);return}if(i===`use:vars`){ot(e,Vt(a,n),r),e.removeAttribute(i);return}if(i===`use:bind`){lt(e,Vt(a,n),r),e.removeAttribute(i);return}if(i.startsWith(_t)){let t=i.slice(1);if(!t)throw Error(`Empty event name in Matrix template`);Ut(e,t,a,n,r);return}j(()=>{let t=Bt(a,n);if(i.startsWith(vt)){let n=i.slice(1);if(!n)throw Error(`Empty property name in Matrix template`);jt(n,String(t??``)),e[n]!==t&&(e[n]=t);return}if(i.startsWith(yt)){let n=i.slice(1);if(!n)throw Error(`Empty boolean attribute name in Matrix template`);e.toggleAttribute(n,!!t);return}if(i===`style`){o=Ht(e,t,o);return}t==null||t===!1?e.removeAttribute(i):(jt(i,String(t)),e.setAttribute(i,String(t))),ft({type:`dom:update`,kind:`attribute`,element:e,name:i})}),r.add(()=>{i.startsWith(vt)?e[i.slice(1)]=void 0:e.removeAttribute(i)})}function Gt(e,t,n,r){let i=t.ownerDocument,a=He(e.strings,i),o=N(r),s=i.createComment(`matrix:start`),c=i.createComment(`matrix:end`);t.insertBefore(s,n),t.insertBefore(c,n);let l=a.template.content.cloneNode(!0),u=a.textBindings.map(({path:e,index:t})=>({node:Ve(l,e),index:t})),d=[],f=a.attributeBindings.map(({path:e,name:t,parts:n})=>({element:Ve(l,e),name:t,parts:n}));Et(e,u,f);try{o.run(()=>{for(let t of f)bt.has(t.name.toLowerCase())&&Wt(t.element,t,e.values,o)}),t.insertBefore(l,c),o.run(()=>{for(let t of u)d.push(Lt(e.values[t.index],t.node.parentNode,t.node,o));for(let t of f)bt.has(t.name.toLowerCase())||Wt(t.element,t,e.values,o)})}catch(e){throw L({dispose:()=>I(d)}),L(o),At(s,c),e}let p=!1;return{firstNode:s,get nodes(){let e=[],t=s.nextSibling;for(;t&&t!==c;)e.push(t),t=t.nextSibling;return e},moveBefore(e){let n=s;for(;n;){let r=n.nextSibling;if(t.insertBefore(n,e),n===c)break;n=r}},dispose(){if(p)return;p=!0;let e;try{I(d)}catch(t){e=t}try{o.dispose()}catch(t){e??=t}try{At(s,c)}catch(t){e??=t}if(e)throw e}}}function Kt(e,t,n,r){let i=N(r),a=xe(),o=t.ownerDocument.createComment(`matrix:keyed:start`),s=t.ownerDocument.createComment(`matrix:keyed:end`),c=new Map,l=[],d=!1,f=e=>a?be(a,e):e();t.insertBefore(o,n),t.insertBefore(s,n);function p(e){let n=o;for(;n;){let r=n.nextSibling;if(t.insertBefore(n,e),n===s)break;n=r}}function m(n){let r=Array.isArray(n)?n:[],a=new Map(c),o=[],d=new Map,f=[],p=[],m=new Set;for(let t of r){let n=e.getKey(t);if(m.has(n))throw u(`Duplicate list key "${String(n)}" detected before reconciliation. Every key in a keyed list must be unique.`,{type:`list:duplicate-key`,key:n}),Error(`Duplicate list key: ${String(n)}`);m.add(n),p.push(n)}try{for(let e=0;e<r.length;e+=1){let n=r[e],c=p[e],l=a.get(c);l?.canUpdate&&!l.canUpdate(n)&&(l.dispose(),l=void 0),l?.update&&(l.update(n)||(l.dispose(),l=void 0)),l||(l=R(n,t,s,i),f.push(l)),d.set(c,l),o.push(l)}let e=s;for(let t=o.length-1;t>=0;--t){let n=o[t];n.moveBefore(e),e=n.firstNode??e}for(let[e,t]of a)d.has(e)||t.dispose();c.clear();for(let[e,t]of d)c.set(e,t);l=o}catch(e){let t=new Set([...a.values(),...f]);throw c.clear(),l=[],L({dispose:()=>I(t)}),e}}try{i.run(()=>{F(e.items)?j(()=>f(()=>m(e.items.value))):f(()=>m(e.items))})}catch(e){throw L({dispose(){I(l),i.dispose()}}),At(o,s),e}return{firstNode:o,get nodes(){let e=[],t=o.nextSibling;for(;t&&t!==s;)e.push(t),t=t.nextSibling;return e},moveBefore(e){p(e)},dispose(){if(d)return;d=!0;let e;try{i.dispose()}catch(t){e=t}try{I(l)}catch(t){e??=t}c.clear(),l=[];try{At(o,s)}catch(t){e??=t}if(e)throw e}}}function qt(e,t,n={}){if(!t||typeof t.insertBefore!=`function`)throw TypeError(`mount() expects a DOM container`);let r=N(),i,a=!1,o=typeof e==`function`?Te(e,n):e;try{r.run(()=>{i=R(o,t,null,r)})}catch(e){throw L(r),e}return{get nodes(){return i.nodes},unmount(){if(a)return;a=!0;let e;try{i.dispose()}catch(t){e=t}try{r.dispose()}catch(t){e??=t}if(e)throw e}}}var Jt=Symbol(`matrix.fragment`),Yt=new Map,Xt=new Map([[`className`,`class`],[`htmlFor`,`for`],[`readOnly`,`readonly`],[`autoFocus`,`autofocus`],[`autoComplete`,`autocomplete`],[`autoPlay`,`autoplay`],[`colSpan`,`colspan`],[`rowSpan`,`rowspan`],[`tabIndex`,`tabindex`]]),Zt=new Set([`checked`,`disabled`,`indeterminate`,`muted`,`selected`,`value`]),Qt=new Set([`area`,`base`,`br`,`col`,`embed`,`hr`,`img`,`input`,`link`,`meta`,`param`,`source`,`track`,`wbr`]);function z(e,t,n){return $t(e,t,n)}function B(e,t,n){return $t(e,t,n)}function $t(e,t,n){let r=t??{},i=n??r.key;if(e===Jt)return r.children??null;if(typeof e==`function`){let t={...r};return delete t.key,Te(e,t,i)}if(typeof e!=`string`||e.length===0)throw TypeError(`jsx() expects an element or Matrix component`);let a=en(e,r);return i!==void 0&&Object.defineProperty(a,"key",{value:i,enumerable:!0}),a}function en(e,t){let n=[],r=[];for(let[e,i]of Object.entries(t))if(e!==`children`&&e!==`key`){if(e===`dangerouslySetInnerHTML`)throw Error(`Matrix does not support dangerouslySetInnerHTML`);n.push(tn(e)),r.push(i)}let i=Object.prototype.hasOwnProperty.call(t,`children`)?[t.children]:[];return P(nn(e,n,i.length),...r,...i)}function tn(e){let t=/^on([A-Z].*)$/.exec(e);if(t){let e=t[1],n=[],r=!0;for(;r;){r=!1;for(let[t,i]of[[`Capture`,`capture`],[`Once`,`once`],[`Passive`,`passive`],[`Prevent`,`prevent`],[`Stop`,`stop`]])if(e.endsWith(t)){e=e.slice(0,-t.length),n.unshift(i),r=!0;break}}return`@${e.toLowerCase()}${n.map(e=>`.${e}`).join(``)}`}return Zt.has(e)?`.${e}`:Xt.get(e)??e}function nn(e,t,n){let r=`${e}\u0000${t.join(``)}\u0000${n}`,i=Yt.get(r);if(i)return i;let a=[`<${e}`];for(let e of t)a[a.length-1]+=` ${e}="`,a.push(`"`);a[a.length-1]+=`>`;for(let e=0;e<n;e+=1)a.push(``);return Qt.has(e.toLowerCase())||(a[a.length-1]+=`</${e}>`),Object.defineProperty(a,"raw",{value:a.slice()}),Yt.set(r,a),a}var rn=new Set([`signal`,`computed`]),an=e=>rn.has(e?.kind),V=(e,t)=>an(e)?e.value:e??t,on=e=>e?.kind===`signal`;function sn(e={}){let{class:t=``,size:n=`1em`,ariaLabel:r}=e;return{iconClass:t?`prism-icon ${t}`:`prism-icon`,size:n,ariaHidden:r===void 0?`true`:`false`,role:r===void 0?void 0:`img`,ariaLabel:r}}function cn(e){let t=[`<svg class="`,`" width="`,`" height="`,`" viewBox="0 0 24 24" fill="none" aria-hidden="`,`" role="`,`" aria-label="`,`" focusable="false">${e}</svg>`];return Object.defineProperty(t,"raw",{value:t.slice()}),t}function H(e){let t=cn(e);return(e={})=>{let{iconClass:n,size:r,ariaHidden:i,role:a,ariaLabel:o}=sn(e);return P(t,n,r,r,i,a,o)}}H(`<circle cx="12" cy="12" r="5.5" fill="currentColor" />`),H(`<path d="M3.5 12s3-5 8.5-5 8.5 5 8.5 5-3 5-8.5 5-8.5-5-8.5-5Z" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round" /><circle cx="12" cy="12" r="2.25" fill="currentColor" />`),H(`<path class="prism-tree-toggle-bar prism-tree-toggle-bar-horizontal" d="M7 12h10" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" /><path class="prism-tree-toggle-bar prism-tree-toggle-bar-vertical" d="M12 7v10" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" />`),H(`<circle cx="12" cy="12" r="3.25" fill="currentColor" />`),H(`<circle cx="12" cy="12" r="2.75" fill="currentColor" />`);var ln=H(`<circle cx="12" cy="12" r="4" fill="currentColor" />`);H(`<circle cx="12" cy="12" r="3" fill="currentColor" />`);var un=H(`<g stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><path d="M12 5v14" /><path d="M5 12h14" /></g>`);H(`<path d="M5 12h14" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" />`);var dn=H(`<g stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><path d="m7 7 10 10" /><path d="m17 7-10 10" /></g>`),fn=H(`<g stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="10.8" cy="10.8" r="5.8" /><path d="m15.2 15.2 4.3 4.3" /></g>`);H(`<path d="M4.5 6h15l-5.8 6.6v4.5L10.3 19v-6.4L4.5 6Z" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round" />`),H(`<g fill="currentColor"><circle cx="6" cy="12" r="1.7" /><circle cx="12" cy="12" r="1.7" /><circle cx="18" cy="12" r="1.7" /></g>`),H(`<path d="M12 19V5m0 0-5 5m5-5 5 5" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" />`),H(`<path d="M12 5v14m0 0-5-5m5 5 5-5" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" />`),H(`<path d="M19 12H5m0 0 5-5m-5 5 5 5" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" />`),H(`<path d="M5 12h14m0 0-5-5m5 5-5 5" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" />`),H(`<path d="m6.5 9.5 5.5 5 5.5-5" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" />`),H(`<path d="m9.5 6.5 5 5.5-5 5.5" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" />`),H(`<g stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"><rect x="4" y="6.5" width="16" height="11" rx="2" /><path d="m5 8 7 5 7-5" /></g>`);var pn=H(`<path d="M6 5.5h12a3 3 0 0 1 3 3v5a3 3 0 0 1-3 3h-5.2L8 19.5v-3H6a3 3 0 0 1-3-3v-5a3 3 0 0 1 3-3Z" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round" />`);H(`<g stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="M18 10a6 6 0 0 0-12 0c0 5-2 5-2 6h16c0-1-2-1-2-6Z" /><path d="M10 20h4" /></g>`),H(`<g fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="m10 14-1.8 1.8a3.3 3.3 0 0 1-4.7-4.7L6 8.6a3.3 3.3 0 0 1 4.7 0" /><path d="m14 10 1.8-1.8a3.3 3.3 0 0 1 4.7 4.7L18 15.4a3.3 3.3 0 0 1-4.7 0" /><path d="m8.5 12h7" /></g>`),H(`<g fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><circle cx="18" cy="5.5" r="2.4" /><circle cx="6" cy="12" r="2.4" /><circle cx="18" cy="18.5" r="2.4" /><path d="m8.2 10.8 7.6-4.1M8.2 13.2l7.6 4.1" /></g>`);var mn=H(`<path d="m4 5 16 7-16 7 3.2-6.1L13 12 7.2 11.1 4 5Z" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round" />`);H(`<path d="m5 12.5 4.5 4.5L19 7" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" />`),H(`<g fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"><path d="m12 4 9 15H3L12 4Z" /><path d="M12 9v4" stroke-linecap="round" /><circle cx="12" cy="16.5" r=".8" fill="currentColor" stroke="none" /></g>`),H(`<g fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round"><circle cx="12" cy="12" r="8.5" /><path d="M12 11v5" /><path d="M12 8h.01" /></g>`),H(`<g fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round"><circle cx="12" cy="12" r="8.5" /><path d="M9.8 9.3a2.4 2.4 0 1 1 3.7 2c-1 .7-1.5 1.1-1.5 2.2" /><path d="M12 16.5h.01" /></g>`),H(`<path d="M20 12a8 8 0 1 1-2.3-5.7" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" />`);var hn=H(`<g fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"><rect x="5" y="10" width="14" height="10" rx="2" /><path d="M8 10V7.8a4 4 0 0 1 8 0V10" /><circle cx="12" cy="15" r="1" fill="currentColor" stroke="none" /></g>`);H(`<g fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"><rect x="5" y="10" width="14" height="10" rx="2" /><path d="M8 10V7.8a4 4 0 0 1 7.1-2.5" /><circle cx="12" cy="15" r="1" fill="currentColor" stroke="none" /></g>`),H(`<g fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"><path d="M7 3.5h7l4 4v13H7v-17Z" /><path d="M14 3.5v4h4" /></g>`),H(`<path d="M3.5 7.5a2 2 0 0 1 2-2h4l2 2h7a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2v-9Z" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round" />`),H(`<g fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"><rect x="4" y="5" width="16" height="14" rx="2" /><circle cx="9" cy="9.5" r="1.4" /><path d="m5 17 4.5-4 3 2.5 2.2-2 4.3 3.5" /></g>`),H(`<g fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M12 4v11m0 0-4-4m4 4 4-4" /><path d="M5 19h14" /></g>`),H(`<g fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M12 20V9m0 0-4 4m4-4 4 4" /><path d="M5 5h14" /></g>`),H(`<g fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"><rect x="8" y="8" width="11" height="12" rx="2" /><path d="M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h2" /></g>`),H(`<g fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"><rect x="4" y="5.5" width="16" height="15" rx="2" /><path d="M8 3.5v4M16 3.5v4M4 10h16" /><path d="M8 14h.01M12 14h.01M16 14h.01M8 17h.01M12 17h.01" stroke-linecap="round" /></g>`),H(`<g fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="8.5" /><path d="M12 7v5l3.5 2" /></g>`),H(`<path d="M19 10.2c0 4.7-7 10.3-7 10.3S5 14.9 5 10.2a7 7 0 1 1 14 0Z" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round" /><circle cx="12" cy="10" r="2.2" fill="currentColor" />`),H(`<g fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="8" r="3.2" /><path d="M5 20a7 7 0 0 1 14 0" /></g>`);var gn=H(`<g fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><circle cx="8.2" cy="8" r="3" /><path d="M3.5 19a4.7 4.7 0 0 1 9.4 0" /><path d="M17 12.5v7M13.5 16h7" /></g>`);H(`<g fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><circle cx="9" cy="8" r="2.8" /><circle cx="16.5" cy="9" r="2.2" /><path d="M3.8 19a5.3 5.3 0 0 1 10.5 0M15 14.7a4.4 4.4 0 0 1 5.2 4.3" /></g>`);var _n=H(`<path d="m12 3 1.2 2.3 2.5.6 2-1.2 1.7 1.7-1.2 2 .6 2.5L21 12l-2.2 1.1-.6 2.5 1.2 2-1.7 1.7-2-1.2-2.5.6L12 21l-1.1-2.3-2.5-.6-2 1.2-1.7-1.7 1.2-2-.6-2.5L3 12l2.3-1.1.6-2.5-1.2-2 1.7-1.7 2 1.2 2.5-.6L12 3Z" fill="none" stroke="currentColor" stroke-width="1.35" stroke-linejoin="round" /><circle cx="12" cy="12" r="2.5" fill="none" stroke="currentColor" stroke-width="1.7" />`);H(`<path d="m12 3 1.6 6.4L20 11l-6.4 1.6L12 19l-1.6-6.4L4 11l6.4-1.6L12 3Z" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round" /><path d="m19 16 .6 2.4L22 19l-2.4.6L19 22l-.6-2.4L16 19l2.4-.6L19 16Z" fill="currentColor" />`),H(`<g fill="none" stroke="currentColor" stroke-width="1.7"><rect x="4" y="4" width="6" height="6" rx="1" /><rect x="14" y="4" width="6" height="6" rx="1" /><rect x="4" y="14" width="6" height="6" rx="1" /><rect x="14" y="14" width="6" height="6" rx="1" /></g>`),H(`<g fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><path d="M8 6h11M8 12h11M8 18h11" /><path d="M4.5 6h.01M4.5 12h.01M4.5 18h.01" /></g>`),H(`<g fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="m9 8-4 4 4 4M15 8l4 4-4 4M13.5 5l-3 14" /></g>`),H(`<g fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><rect x="4" y="5" width="16" height="14" rx="2" /><path d="m8 10 2 2-2 2M13 14h3" /></g>`),H(`<g fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="M4 5 20 19" /><path d="M10.5 7.2A9.5 9.5 0 0 1 12 7c5.5 0 8.5 5 8.5 5a15 15 0 0 1-3.1 3.4M6.6 9.1C4.7 10.4 3.5 12 3.5 12s3 5 8.5 5c.7 0 1.4-.1 2-.2" /></g>`);var vn=0,yn=new Set([`success`,`info`,`warning`,`error`]);function bn(e={}){let{ariaLabel:t,children:n,class:r=``,dismissible:i=!1,id:a,onDismiss:o,role:s,title:c,tone:l=`info`}=e,u=M(()=>{let e=V(l,`info`);return yn.has(e)?e:`info`}),d=M(()=>V(c)),f=n??e.description,p=`${a??`prism-alert-${++vn}`}-title`,m=`${a??`prism-alert-${vn}`}-description`,h=s??(u.value===`error`||u.value===`warning`?`alert`:`status`),g=d.value!==void 0&&d.value!==null&&d.value!==``,_=f!=null&&f!==``;return P`
    <div
      class="prism-alert prism-alert-${u.value} ${r}"
      id="${a??``}"
      role="${h}"
      aria-label="${t}"
      aria-labelledby="${g?p:void 0}"
      aria-describedby="${_?m:void 0}"
    >
      <span class="prism-alert-icon" aria-hidden="true">${u.value===`success`?`✓`:u.value===`error`||u.value===`warning`?`!`:`i`}</span>
      <div class="prism-alert-body">
        ${g?P`<div class="prism-alert-title" id="${p}">${d.value}</div>`:``}
        ${_?P`<div class="prism-alert-description" id="${m}">${f}</div>`:``}
      </div>
      ${i?P`
        <button class="prism-alert-dismiss" type="button" aria-label="Dismiss" @click=${o}>
          ${dn({size:16})}
        </button>
      `:``}
    </div>
  `}var xn=(e,t=!1)=>!!V(e,t),Sn=(e,t,n)=>{let r=V(e,n);return t.has(r)?r:n},Cn=new Set([`small`,`medium`,`large`]),wn=e=>String(e??`?`).trim().split(/\s+/).filter(Boolean).slice(0,2).map(e=>e[0]).join(``).toUpperCase()||`?`;function Tn(e={}){let{alt:t,class:n=``,name:r,size:i=`medium`,src:a,status:o,statusSize:s=`large`,showStatus:c=!0,variant:l=`circle`}=e,u=M(()=>V(r,`User`)),d=M(()=>V(a)),f=M(()=>V(o)),p=M(()=>V(i,`medium`)),m=M(()=>V(l,`circle`)),h=M(()=>{let e=String(V(s,`large`));return Cn.has(e)?e:`large`}),g=M(()=>!!V(c,!0)),_=M(()=>V(t));return P`
    <span class="prism-avatar prism-avatar-${p} prism-avatar-${m} ${n}" aria-label="${u}">
      ${M(()=>d.value?P`<img src="${d}" alt="${_.value??u.value}" loading="lazy">`:P`<span aria-hidden="true">${wn(u.value)}</span>`)}
      ${M(()=>g.value&&f.value?P`<span class="prism-avatar-status prism-avatar-status-${f} prism-avatar-status-size-${h}" aria-label="${f}"></span>`:null)}
    </span>
  `}var En=`prism-badge`;function Dn(e={}){let{value:t,children:n=[],tone:r=`neutral`,size:i=`medium`,pulseOnChange:a=!1,class:o=``,ariaLabel:s}=e,c=t===void 0?n:t,l=an(r)?r:r||`neutral`,u=an(i)?i:i||`medium`,d=e=>P`<span class="${En} ${En}-${l} ${En}-${u} ${e?`${En}-pulse`:``} ${o}" role="${s?`img`:void 0}" aria-label="${s}">${c}</span>`;if(!a||!an(c))return d(!1);let f,p=!1;return M(()=>{let e=c.value,t=p&&!Object.is(e,f);return f=e,p=!0,d(t)})}var U=`prism-button`,On=new Set([`primary`,`secondary`,`tertiary`,`error`,`warning`,`information`,`success`]),kn=new Set([`small`,`medium`,`large`]),An=new Set([`rounded`,`pill`,`square`]),jn=new Set([`start`,`end`]),Mn=new Set([`cobalt`,`iris`,`teal`]),W=V;function G(e={}){let{children:t=[],label:n,showLabel:r=!0,icon:i,iconPosition:a=`start`,class:o=``,id:s,type:c=`button`,name:l,value:u,variant:d=`primary`,size:f=`medium`,shape:p=`rounded`,palette:m,fullWidth:h=!1,loading:g=!1,loadingLabel:_=`Loading`,pressed:v,disabled:y=!1,ariaLabel:b,title:x,onClick:S,onFocus:C,onBlur:w}=e,T=M(()=>{let e=Sn(d,On,`primary`),t=Sn(f,kn,`medium`),n=Sn(p,An,`rounded`),i=xn(r,!0);return[U,`${U}-${e}`,`${U}-${t}`,`${U}-${n}`,i?``:`${U}-icon-only`,W(h,!1)?`${U}-full-width`:``,W(g,!1)?`${U}-loading`:``,W(v,!1)?`${U}-pressed`:``,o].filter(Boolean).join(` `)}),ee=M(()=>{let e=W(r,!0),o=jn.has(W(a))?W(a):`start`,s=W(g,!1)?W(_,`Loading`):n===void 0?t:n,c=W(g,!1)?P`<span class="${U}-spinner" aria-hidden="true"></span>`:W(i),l=c==null?null:P`<span class="${U}-icon" aria-hidden="true">${c}</span>`,u=e?P`<span class="${U}-label">${s}</span>`:null;return o===`end`&&e?P`${u}${l}`:P`${l}${u}`}),E=M(()=>W(y,!1)||W(g,!1)),D=M(()=>String(W(g,!1))),te=M(()=>v===void 0?void 0:String(W(v,!1)));return P`<button type="${c}" class="${T}" id="${s}" name="${l}" value="${u}" title="${x}" data-prism-palette="${M(()=>{let e=W(m);return Mn.has(e)?e:void 0})}" aria-label="${M(()=>{let e=W(b);if(e!==void 0)return e;if(W(g,!1))return W(_,`Loading`);if(!W(r,!0)){let e=W(n===void 0?t:n);return typeof e==`string`||typeof e==`number`?String(e):`Button`}let i=W(n===void 0?t:n);return typeof i==`string`||typeof i==`number`?void 0:`Button`})}" aria-busy="${D}" aria-pressed="${te}" ?disabled=${E} @click=${e=>{let t=W(S);typeof t==`function`&&t(e)}} @focus=${C} @blur=${w}>${ee}</button>`}function Nn(e={}){let{action:t,children:n,class:r=``,description:i,icon:a,onRetry:o,retryLabel:s=`Try again`,status:c=`empty`,title:l=`Nothing here yet`}=e,u=M(()=>V(c,`empty`)),d=M(()=>V(l,`Nothing here yet`)),f=M(()=>V(i??n)),p=M(()=>typeof t==`function`?t():t),m=o?P`<button class="prism-button prism-button-secondary" type="button" @click=${o}>${s}</button>`:``;return P`
    <section class="prism-empty-state prism-empty-state-${u.value} ${r}" role="status">
      ${a?P`<div class="prism-empty-state-icon" aria-hidden="true">${a}</div>`:``}
      <h3>${d.value}</h3>
      ${f.value?P`<p>${f.value}</p>`:``}
      ${p.value||m?P`<div class="prism-empty-state-actions">${p.value}${m}</div>`:``}
    </section>
  `}var K=`prism-popup`,Pn=new Set([`small`,`medium`,`large`,`full`]),Fn=new Set([`center`,`top`,`bottom`]),In=0,Ln=0,Rn=``,q=V;function zn(){typeof document<`u`&&document.body&&(Ln===0&&(Rn=document.body.style.overflow,document.body.style.overflow=`hidden`),Ln+=1)}function Bn(){typeof document<`u`&&document.body&&Ln!==0&&(--Ln,Ln===0&&(document.body.style.overflow=Rn,Rn=``))}function Vn(e){return typeof requestAnimationFrame==`function`?requestAnimationFrame(e):setTimeout(e,0)}function Hn(e){e!=null&&(typeof cancelAnimationFrame==`function`?cancelAnimationFrame(e):clearTimeout(e))}function Un(e){if(e.hidden||e.getAttribute(`aria-hidden`)===`true`)return!1;let t=globalThis.getComputedStyle?.(e);return!t||t.display!==`none`&&t.visibility!==`hidden`}function Wn(e){return e?[...e.querySelectorAll(`button:not(:disabled), [href], input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [contenteditable="true"], [tabindex]:not([tabindex="-1"])`)].filter(Un):[]}function Gn({openValue:e,instanceId:t,restoreFocus:n,focusState:r}){return De(()=>{let i=null,a=null,o=!1,s=!1,c=j(()=>{if(!e.value){if(Hn(i),i=null,s&&q(n,!0)&&r.returnFocusTarget?.focus){let e=r.returnFocusTarget;a=Vn(()=>{a=null,e.isConnected!==!1&&e.focus()})}s=!1,o&&=(Bn(),!1);return}s||=(Hn(a),a=null,r.returnFocusTarget=typeof document>`u`?void 0:document.activeElement,!0),o||=(zn(),!0),Hn(i),i=Vn(()=>{if(i=null,!e.value||typeof document>`u`)return;let n=document.getElementById(t);(Wn(n)[0]??n)?.focus()})},{flush:`microtask`});return()=>{c(),Hn(i),Hn(a),o&&Bn()}}),null}function Kn(e={}){let{open:t=!1,title:n,eyebrow:r,children:i,footer:a,size:o=`medium`,placement:s=`center`,showClose:c=!0,closeOnBackdrop:l=!0,closeOnEscape:u=!0,restoreFocus:d=!0,class:f=``,id:p,ariaLabel:m=`Dialog`,ariaDescription:h,onClose:g}=e,_=on(t)?t:O(!!q(t,!1)),v=p??`prism-popup-${In+=1}`,y=`${v}-title`,b=`${v}-description`,x=Te(Gn,{openValue:_,instanceId:v,restoreFocus:d,focusState:{}}),S=(e=`programmatic`,t)=>{_.value=!1,g?.(e,t)},C=e=>{if(e.key===`Escape`&&q(u,!0)){e.preventDefault(),S(`escape`,e);return}if(e.key!==`Tab`)return;let t=e.currentTarget.querySelector(`.${K}-panel`),n=Wn(t);if(n.length===0){e.preventDefault(),t?.focus();return}let r=n[0],i=n[n.length-1];e.shiftKey&&document.activeElement===r?(e.preventDefault(),i.focus()):!e.shiftKey&&document.activeElement===i&&(e.preventDefault(),r.focus())},w=e=>typeof e==`function`?e({close:S}):e;return P`${x}${M(()=>{if(!_.value)return null;let e=Pn.has(q(o))?q(o):`medium`,t=Fn.has(q(s))?q(s):`center`,u=q(n),d=u!=null&&String(u).trim()!==``,p=String(q(m,`Dialog`)??``).trim()||`Dialog`,g=d||r!==void 0||q(c,!0),x=[`${K}-panel`,`${K}-${e}`,f].filter(Boolean).join(` `);return P`
      <div class="${K}-layer ${K}-placement-${t}" @keydown=${C}>
        <span class="${K}-backdrop" aria-hidden="true" @click=${e=>{q(l,!0)&&S(`backdrop`,e)}}></span>
        <section
          class="${x}"
          id="${v}"
          role="dialog"
          aria-modal="true"
          aria-label="${d?void 0:p}"
          aria-labelledby="${d?y:void 0}"
          aria-describedby="${h===void 0?void 0:b}"
          tabindex="-1"
          ?autofocus=${!q(c,!0)}
        >
          ${g?P`
            <header class="${K}-header">
              <div class="${K}-heading">
                ${r===void 0?null:P`<span class="${K}-eyebrow">${r}</span>`}
                ${d?P`<strong class="${K}-title" id="${y}">${n}</strong>`:null}
                ${h===void 0?null:P`<span class="${K}-description" id="${b}">${h}</span>`}
              </div>
              ${q(c,!0)?P`<button type="button" class="${K}-close" aria-label="Close popup" autofocus @click=${e=>S(`close-button`,e)}>${dn({size:`1em`})}</button>`:null}
            </header>
          `:null}
          <div class="${K}-body">${w(i)}</div>
          ${a==null?null:P`<footer class="${K}-footer">${w(a)}</footer>`}
        </section>
      </div>
    `})}`}var qn=`text-field`,Jn=new Set([`small`,`medium`,`large`]),Yn=0,Xn=e=>e!=null&&e!==``;function Zn(e={}){let{value:t=``,onInput:n,onChange:r,onFocus:i,onBlur:a,id:o,name:s,placeholder:c,disabled:l=!1,required:u=!1,size:d=`medium`,type:f=`text`,autocomplete:p,inputMode:m,maxLength:h,minLength:g,pattern:_,readOnly:v=!1,ariaLabel:y,ariaDescription:b,ariaDescribedBy:x,ariaInvalid:S,error:C,class:w=``,style:T}=e,ee=M(()=>Jn.has(String(V(d,`medium`)))?String(V(d,`medium`)):`medium`),E=M(()=>String(V(f,`text`))),D=M(()=>V(C)),te=M(()=>Xn(D.value)?D.value:V(b)),ne=M(()=>Xn(D.value)),re=M(()=>Xn(te.value)),ie=o?`${o}-message`:`prism-text-field-${Yn+=1}-message`,O=M(()=>[V(x),re.value?ie:void 0].filter(Boolean).join(` `)||void 0),k=M(()=>{let e=V(S);return e==null?ne.value:!!e}),A=M(()=>[qn,`${qn}-${ee.value}`,k.value?`${qn}-invalid`:``,w].filter(Boolean).join(` `)),ae=M(()=>V(T)),oe=on(t)?null:P`<input
      type="${E}"
      class="${A}"
      id="${o}"
      name="${s}"
      placeholder="${c}"
      autocomplete="${p}"
      inputmode="${m}"
      maxlength="${h}"
      minlength="${g}"
      pattern="${_}"
      aria-label="${y}"
      aria-describedby="${O}"
      aria-invalid="${M(()=>k.value?`true`:void 0)}"
      .value=${an(t)?t:String(t??``)}
      ?disabled=${l}
      ?required=${u}
      ?readonly=${v}
      style="${ae}"
      @input=${n}
      @change=${r}
      @focus=${i}
      @blur=${a}
    >`;return P`${on(t)?P`<input
      type="${E}"
      class="${A}"
      id="${o}"
      name="${s}"
      placeholder="${c}"
      autocomplete="${p}"
      inputmode="${m}"
      maxlength="${h}"
      minlength="${g}"
      pattern="${_}"
      aria-label="${y}"
      aria-describedby="${O}"
      aria-invalid="${M(()=>k.value?`true`:void 0)}"
      use:bind=${t}
      ?disabled=${l}
      ?required=${u}
      ?readonly=${v}
      style="${ae}"
      @input=${n}
      @change=${r}
      @focus=${i}
      @blur=${a}
    >`:oe}${M(()=>{let e=te.value;return Xn(e)?P`<span id="${ie}" class="${qn}-message ${ne.value?`${qn}-message-error`:``}" role="${ne.value?`alert`:void 0}">${e}</span>`:null})}`}var Qn=Object.freeze({cobalt:Object.freeze({"--prism-button-primary-border":`rgb(54 87 214 / 20%)`,"--prism-button-primary-background":`linear-gradient(135deg, #3657d6, #4e73ea)`,"--prism-button-primary-background-hover":`linear-gradient(135deg, #2f4dc5, #4569df)`,"--prism-button-primary-background-active":`linear-gradient(135deg, #2842ae, #3a58c7)`,"--prism-button-primary-shadow":`0 .45rem 1rem rgb(54 87 214 / 24%), inset 0 1px 0 rgb(255 255 255 / 20%)`,"--prism-button-primary-shadow-hover":`0 .6rem 1.2rem rgb(54 87 214 / 32%), inset 0 1px 0 rgb(255 255 255 / 24%)`,"--prism-button-primary-shadow-active":`0 .15rem .35rem rgb(54 87 214 / 20%), inset 0 2px 5px rgb(40 66 174 / 30%)`,"--prism-button-secondary-border":`rgb(112 128 153 / 22%)`,"--prism-button-secondary-background":`linear-gradient(135deg, #708099, #8594ab)`,"--prism-button-secondary-background-hover":`linear-gradient(135deg, #62718a, #77859b)`,"--prism-button-secondary-background-active":`linear-gradient(135deg, #556178, #677387)`,"--prism-button-secondary-shadow":`0 .36rem .9rem rgb(112 128 153 / 20%), inset 0 1px 0 rgb(255 255 255 / 16%)`,"--prism-button-secondary-shadow-hover":`0 .52rem 1.1rem rgb(112 128 153 / 26%), inset 0 1px 0 rgb(255 255 255 / 20%)`,"--prism-button-secondary-shadow-active":`0 .12rem .32rem rgb(112 128 153 / 18%), inset 0 2px 5px rgb(65 79 102 / 24%)`,"--prism-button-tertiary-border":`rgb(240 138 107 / 22%)`,"--prism-button-tertiary-background":`linear-gradient(135deg, #f08a6b, #f4a17e)`,"--prism-button-tertiary-background-hover":`linear-gradient(135deg, #e57d5d, #ef9471)`,"--prism-button-tertiary-background-active":`linear-gradient(135deg, #cf684b, #de7d5e)`,"--prism-button-tertiary-shadow":`0 .42rem .96rem rgb(240 138 107 / 22%), inset 0 1px 0 rgb(255 255 255 / 18%)`,"--prism-button-tertiary-shadow-hover":`0 .56rem 1.14rem rgb(240 138 107 / 28%), inset 0 1px 0 rgb(255 255 255 / 22%)`,"--prism-button-tertiary-shadow-active":`0 .12rem .32rem rgb(240 138 107 / 18%), inset 0 2px 5px rgb(150 80 55 / 26%)`}),iris:Object.freeze({"--prism-button-primary-border":`rgb(109 94 247 / 20%)`,"--prism-button-primary-background":`linear-gradient(135deg, #6d5ef7, #8a76ff)`,"--prism-button-primary-background-hover":`linear-gradient(135deg, #5f4eeb, #7b66f8)`,"--prism-button-primary-background-active":`linear-gradient(135deg, #5343d4, #6c57e4)`,"--prism-button-primary-shadow":`0 .45rem 1rem rgb(109 94 247 / 24%), inset 0 1px 0 rgb(255 255 255 / 20%)`,"--prism-button-primary-shadow-hover":`0 .6rem 1.2rem rgb(109 94 247 / 32%), inset 0 1px 0 rgb(255 255 255 / 24%)`,"--prism-button-primary-shadow-active":`0 .15rem .35rem rgb(109 94 247 / 20%), inset 0 2px 5px rgb(65 52 165 / 30%)`,"--prism-button-secondary-border":`rgb(100 116 139 / 22%)`,"--prism-button-secondary-background":`linear-gradient(135deg, #64748b, #7a889c)`,"--prism-button-secondary-background-hover":`linear-gradient(135deg, #56657b, #6c7a8e)`,"--prism-button-secondary-background-active":`linear-gradient(135deg, #4a576b, #5e6c80)`,"--prism-button-secondary-shadow":`0 .36rem .9rem rgb(100 116 139 / 20%), inset 0 1px 0 rgb(255 255 255 / 16%)`,"--prism-button-secondary-shadow-hover":`0 .52rem 1.1rem rgb(100 116 139 / 26%), inset 0 1px 0 rgb(255 255 255 / 20%)`,"--prism-button-secondary-shadow-active":`0 .12rem .32rem rgb(100 116 139 / 18%), inset 0 2px 5px rgb(51 65 85 / 24%)`,"--prism-button-tertiary-border":`rgb(242 107 94 / 22%)`,"--prism-button-tertiary-background":`linear-gradient(135deg, #f26b5e, #f58a74)`,"--prism-button-tertiary-background-hover":`linear-gradient(135deg, #e66054, #ee7d68)`,"--prism-button-tertiary-background-active":`linear-gradient(135deg, #cf554b, #dd6d5b)`,"--prism-button-tertiary-shadow":`0 .42rem .96rem rgb(242 107 94 / 22%), inset 0 1px 0 rgb(255 255 255 / 18%)`,"--prism-button-tertiary-shadow-hover":`0 .56rem 1.14rem rgb(242 107 94 / 28%), inset 0 1px 0 rgb(255 255 255 / 22%)`,"--prism-button-tertiary-shadow-active":`0 .12rem .32rem rgb(242 107 94 / 18%), inset 0 2px 5px rgb(145 64 50 / 26%)`}),teal:Object.freeze({"--prism-button-primary-border":`rgb(15 118 110 / 22%)`,"--prism-button-primary-background":`linear-gradient(135deg, #0f766e, #159b91)`,"--prism-button-primary-background-hover":`linear-gradient(135deg, #0d675f, #11867f)`,"--prism-button-primary-background-active":`linear-gradient(135deg, #0b5852, #0e726b)`,"--prism-button-primary-shadow":`0 .45rem 1rem rgb(15 118 110 / 24%), inset 0 1px 0 rgb(255 255 255 / 20%)`,"--prism-button-primary-shadow-hover":`0 .6rem 1.2rem rgb(15 118 110 / 32%), inset 0 1px 0 rgb(255 255 255 / 24%)`,"--prism-button-primary-shadow-active":`0 .15rem .35rem rgb(15 118 110 / 20%), inset 0 2px 5px rgb(12 78 74 / 30%)`,"--prism-button-secondary-border":`rgb(107 124 147 / 22%)`,"--prism-button-secondary-background":`linear-gradient(135deg, #6b7c93, #8190a5)`,"--prism-button-secondary-background-hover":`linear-gradient(135deg, #5e7087, #74849a)`,"--prism-button-secondary-background-active":`linear-gradient(135deg, #526278, #66768d)`,"--prism-button-secondary-shadow":`0 .36rem .9rem rgb(107 124 147 / 20%), inset 0 1px 0 rgb(255 255 255 / 16%)`,"--prism-button-secondary-shadow-hover":`0 .52rem 1.1rem rgb(107 124 147 / 26%), inset 0 1px 0 rgb(255 255 255 / 20%)`,"--prism-button-secondary-shadow-active":`0 .12rem .32rem rgb(107 124 147 / 18%), inset 0 2px 5px rgb(57 69 86 / 24%)`,"--prism-button-tertiary-border":`rgb(245 158 11 / 22%)`,"--prism-button-tertiary-background":`linear-gradient(135deg, #f59e0b, #f7b84a)`,"--prism-button-tertiary-background-hover":`linear-gradient(135deg, #e39107, #efae36)`,"--prism-button-tertiary-background-active":`linear-gradient(135deg, #c97c06, #d89522)`,"--prism-button-tertiary-shadow":`0 .42rem .96rem rgb(245 158 11 / 22%), inset 0 1px 0 rgb(255 255 255 / 18%)`,"--prism-button-tertiary-shadow-hover":`0 .56rem 1.14rem rgb(245 158 11 / 28%), inset 0 1px 0 rgb(255 255 255 / 22%)`,"--prism-button-tertiary-shadow-active":`0 .12rem .32rem rgb(245 158 11 / 18%), inset 0 2px 5px rgb(146 94 20 / 26%)`})}),J=Object.freeze({colors:Object.freeze({page:`#f5f7fb`,surface:`#ffffff`,surfaceGlass:`rgb(255 255 255 / 84%)`,surfaceCard:`rgb(255 255 255 / 82%)`,surfaceTint:`#f8faff`,surfaceRaised:`#ffffff`,surfaceHover:`#f8faff`,white:`#ffffff`,whiteStrong:`rgb(255 255 255 / 90%)`,whiteSoft:`rgb(255 255 255 / 24%)`,whiteFaint:`rgb(255 255 255 / 20%)`,whiteTint:`rgb(255 255 255 / 12%)`,ink:`#1d2638`,text:`#53617a`,textMuted:`#6b7892`,textSoft:`#7b879e`,textSubtle:`#8a95a8`,placeholder:`#aab3c2`,border:`#e5e9f1`,borderStrong:`#cbd3e1`,borderInput:`#dbe1eb`,borderFaint:`#edf0f5`,accent:`#ef685a`,accentBright:`#f26b5e`,accentGlow:`rgb(242 107 94 / 12%)`,accentSoft:`#fff0ed`,accentHover:`#dc594c`,action:`#3657d6`,actionEnd:`#4e73ea`,actionHover:`#2f4dc5`,actionHoverEnd:`#4569df`,actionActive:`#2842ae`,actionActiveEnd:`#3a58c7`,primary:`#3657d6`,primaryEnd:`#4e73ea`,primaryHover:`#2f4dc5`,primaryHoverEnd:`#4569df`,primaryActive:`#2842ae`,primaryActiveEnd:`#3a58c7`,secondary:`#708099`,secondaryEnd:`#8594ab`,secondaryHover:`#62718a`,secondaryHoverEnd:`#77859b`,secondaryActive:`#556178`,secondaryActiveEnd:`#677387`,tertiary:`#f08a6b`,tertiaryEnd:`#f4a17e`,tertiaryHover:`#e57d5d`,tertiaryHoverEnd:`#ef9471`,tertiaryActive:`#cf684b`,tertiaryActiveEnd:`#de7d5e`,error:`#dc2626`,errorEnd:`#ef4444`,errorHover:`#c81f1f`,errorHoverEnd:`#df3b3b`,errorActive:`#b91c1c`,errorActiveEnd:`#cd3030`,warning:`#d97706`,warningEnd:`#f59e0b`,warningHover:`#c56b05`,warningHoverEnd:`#e68f08`,warningActive:`#a95b05`,warningActiveEnd:`#c97908`,information:`#0284c7`,informationEnd:`#0ea5e9`,informationHover:`#036fa8`,informationHoverEnd:`#0b92d0`,informationActive:`#075985`,informationActiveEnd:`#0478b1`,actionPreview:`#7787a4`,actionPreviewHover:`#657593`,actionGlow:`rgb(54 87 214 / 24%)`,actionHoverGlow:`rgb(54 87 214 / 32%)`,actionActiveShadow:`rgb(40 66 174 / 30%)`,actionFocusGlow:`rgb(54 87 214 / 24%)`,focus:`#4e73ea`,focusGlow:`rgb(78 115 234 / 14%)`,focusStrongGlow:`rgb(78 115 234 / 20%)`,invalidGlow:`rgb(239 104 90 / 12%)`,success:`#3c9b7a`,successBright:`#53c69d`,successGlow:`rgb(83 198 157 / 15%)`,previewGlow:`rgb(166 142 241 / 12%)`,lavenderBorder:`#d9d1ff`,lavenderSurface:`#f0edff`,mintBorder:`#c7ebdf`,mintSurface:`#eaf9f4`,peachBorder:`#f5d5c9`,peachSurface:`#fff1eb`}),fontSizes:Object.freeze({micro:`.7rem`,label:`.72rem`,small:`.75rem`,bodySmall:`.78rem`,compact:`.76rem`,body:`.82rem`,copy:`.9rem`,cardCopy:`.92rem`,ui:`.8rem`,lead:`1.05rem`,heading:`1.45rem`,hero:`clamp(3rem, 7vw, 5.8rem)`,detailHero:`clamp(3rem, 7vw, 5rem)`}),radii:Object.freeze({control:`.6rem`,card:`1.25rem`,preview:`.85rem`,surface:`1rem`}),shadows:Object.freeze({card:`0 .9rem 2.5rem rgb(37 49 78 / 6%)`,action:`0 .45rem 1rem rgb(54 87 214 / 24%), inset 0 1px 0 rgb(255 255 255 / 20%)`,actionHover:`0 .6rem 1.2rem rgb(54 87 214 / 32%), inset 0 1px 0 rgb(255 255 255 / 24%)`,actionActive:`0 .15rem .35rem rgb(54 87 214 / 20%), inset 0 2px 5px rgb(40 66 174 / 30%)`,secondary:`0 .36rem .9rem rgb(112 128 153 / 20%), inset 0 1px 0 rgb(255 255 255 / 16%)`,secondaryHover:`0 .52rem 1.1rem rgb(112 128 153 / 26%), inset 0 1px 0 rgb(255 255 255 / 20%)`,secondaryActive:`0 .12rem .32rem rgb(112 128 153 / 18%), inset 0 2px 5px rgb(65 79 102 / 24%)`,tertiary:`0 .42rem .96rem rgb(240 138 107 / 22%), inset 0 1px 0 rgb(255 255 255 / 18%)`,tertiaryHover:`0 .56rem 1.14rem rgb(240 138 107 / 28%), inset 0 1px 0 rgb(255 255 255 / 22%)`,tertiaryActive:`0 .12rem .32rem rgb(240 138 107 / 18%), inset 0 2px 5px rgb(150 80 55 / 26%)`,error:`0 .4rem .92rem rgb(220 38 38 / 22%), inset 0 1px 0 rgb(255 255 255 / 16%)`,errorHover:`0 .54rem 1.08rem rgb(220 38 38 / 28%), inset 0 1px 0 rgb(255 255 255 / 22%)`,errorActive:`0 .12rem .32rem rgb(220 38 38 / 18%), inset 0 2px 5px rgb(127 29 29 / 26%)`,warning:`0 .4rem .92rem rgb(217 119 6 / 22%), inset 0 1px 0 rgb(255 255 255 / 16%)`,warningHover:`0 .54rem 1.08rem rgb(217 119 6 / 28%), inset 0 1px 0 rgb(255 255 255 / 22%)`,warningActive:`0 .12rem .32rem rgb(217 119 6 / 18%), inset 0 2px 5px rgb(120 53 15 / 26%)`,information:`0 .4rem .92rem rgb(2 132 199 / 22%), inset 0 1px 0 rgb(255 255 255 / 16%)`,informationHover:`0 .54rem 1.08rem rgb(2 132 199 / 28%), inset 0 1px 0 rgb(255 255 255 / 22%)`,informationActive:`0 .12rem .32rem rgb(2 132 199 / 18%), inset 0 2px 5px rgb(12 74 110 / 26%)`,success:`0 .4rem .92rem rgb(60 155 122 / 22%), inset 0 1px 0 rgb(255 255 255 / 16%)`,successHover:`0 .54rem 1.08rem rgb(60 155 122 / 28%), inset 0 1px 0 rgb(255 255 255 / 22%)`,successActive:`0 .12rem .32rem rgb(60 155 122 / 18%), inset 0 2px 5px rgb(24 101 73 / 26%)`})}),$n=Object.freeze({"--prism-button-padding":`.6rem .8rem`,"--prism-button-border-width":`1px`,"--prism-button-radius":J.radii.control,"--prism-button-font-size":J.fontSizes.small,"--prism-button-font-weight":`750`,"--prism-button-transform-hover":`translateY(-1px)`,"--prism-button-transform-active":`translateY(1px) scale(.98)`,"--prism-button-focus-outline":`3px solid ${J.colors.actionFocusGlow}`,"--prism-button-focus-offset":`3px`,"--prism-button-disabled-opacity":`.55`,"--prism-button-primary-text":J.colors.white,"--prism-button-primary-border":J.colors.whiteSoft,"--prism-button-primary-background":`linear-gradient(135deg, ${J.colors.primary}, ${J.colors.primaryEnd})`,"--prism-button-primary-background-hover":`linear-gradient(135deg, ${J.colors.primaryHover}, ${J.colors.primaryHoverEnd})`,"--prism-button-primary-background-active":`linear-gradient(135deg, ${J.colors.primaryActive}, ${J.colors.primaryActiveEnd})`,"--prism-button-primary-shadow":J.shadows.action,"--prism-button-primary-shadow-hover":J.shadows.actionHover,"--prism-button-primary-shadow-active":J.shadows.actionActive,"--prism-button-secondary-text":J.colors.white,"--prism-button-secondary-border":`rgb(100 116 139 / 22%)`,"--prism-button-secondary-background":`linear-gradient(135deg, ${J.colors.secondary}, ${J.colors.secondaryEnd})`,"--prism-button-secondary-background-hover":`linear-gradient(135deg, ${J.colors.secondaryHover}, ${J.colors.secondaryHoverEnd})`,"--prism-button-secondary-background-active":`linear-gradient(135deg, ${J.colors.secondaryActive}, ${J.colors.secondaryActiveEnd})`,"--prism-button-secondary-shadow":J.shadows.secondary,"--prism-button-secondary-shadow-hover":J.shadows.secondaryHover,"--prism-button-secondary-shadow-active":J.shadows.secondaryActive,"--prism-button-tertiary-text":J.colors.white,"--prism-button-tertiary-border":`rgb(231 111 81 / 22%)`,"--prism-button-tertiary-background":`linear-gradient(135deg, ${J.colors.tertiary}, ${J.colors.tertiaryEnd})`,"--prism-button-tertiary-background-hover":`linear-gradient(135deg, ${J.colors.tertiaryHover}, ${J.colors.tertiaryHoverEnd})`,"--prism-button-tertiary-background-active":`linear-gradient(135deg, ${J.colors.tertiaryActive}, ${J.colors.tertiaryActiveEnd})`,"--prism-button-tertiary-shadow":J.shadows.tertiary,"--prism-button-tertiary-shadow-hover":J.shadows.tertiaryHover,"--prism-button-tertiary-shadow-active":J.shadows.tertiaryActive,"--prism-button-error-text":J.colors.white,"--prism-button-error-border":`rgb(220 38 38 / 22%)`,"--prism-button-error-background":`linear-gradient(135deg, ${J.colors.error}, ${J.colors.errorEnd})`,"--prism-button-error-background-hover":`linear-gradient(135deg, ${J.colors.errorHover}, ${J.colors.errorHoverEnd})`,"--prism-button-error-background-active":`linear-gradient(135deg, ${J.colors.errorActive}, ${J.colors.errorActiveEnd})`,"--prism-button-error-shadow":J.shadows.error,"--prism-button-error-shadow-hover":J.shadows.errorHover,"--prism-button-error-shadow-active":J.shadows.errorActive,"--prism-button-warning-text":J.colors.white,"--prism-button-warning-border":`rgb(217 119 6 / 22%)`,"--prism-button-warning-background":`linear-gradient(135deg, ${J.colors.warning}, ${J.colors.warningEnd})`,"--prism-button-warning-background-hover":`linear-gradient(135deg, ${J.colors.warningHover}, ${J.colors.warningHoverEnd})`,"--prism-button-warning-background-active":`linear-gradient(135deg, ${J.colors.warningActive}, ${J.colors.warningActiveEnd})`,"--prism-button-warning-shadow":J.shadows.warning,"--prism-button-warning-shadow-hover":J.shadows.warningHover,"--prism-button-warning-shadow-active":J.shadows.warningActive,"--prism-button-information-text":J.colors.white,"--prism-button-information-border":`rgb(2 132 199 / 22%)`,"--prism-button-information-background":`linear-gradient(135deg, ${J.colors.information}, ${J.colors.informationEnd})`,"--prism-button-information-background-hover":`linear-gradient(135deg, ${J.colors.informationHover}, ${J.colors.informationHoverEnd})`,"--prism-button-information-background-active":`linear-gradient(135deg, ${J.colors.informationActive}, ${J.colors.informationActiveEnd})`,"--prism-button-information-shadow":J.shadows.information,"--prism-button-information-shadow-hover":J.shadows.informationHover,"--prism-button-information-shadow-active":J.shadows.informationActive,"--prism-button-success-text":J.colors.white,"--prism-button-success-border":`rgb(60 155 122 / 22%)`,"--prism-button-success-background":`linear-gradient(135deg, ${J.colors.success}, ${J.colors.successBright})`,"--prism-button-success-background-hover":`linear-gradient(135deg, #318b6d, ${J.colors.success})`,"--prism-button-success-background-active":`linear-gradient(135deg, #27765c, #3b8d71)`,"--prism-button-success-shadow":J.shadows.success,"--prism-button-success-shadow-hover":J.shadows.successHover,"--prism-button-success-shadow-active":J.shadows.successActive,"--prism-color-page":J.colors.page,"--prism-color-surface":J.colors.surface,"--prism-color-surface-glass":J.colors.surfaceGlass,"--prism-color-surface-card":J.colors.surfaceCard,"--prism-color-surface-tint":J.colors.surfaceTint,"--prism-color-surface-raised":J.colors.surfaceRaised,"--prism-color-surface-hover":J.colors.surfaceHover,"--prism-color-white":J.colors.white,"--prism-color-white-strong":J.colors.whiteStrong,"--prism-color-white-soft":J.colors.whiteSoft,"--prism-color-white-faint":J.colors.whiteFaint,"--prism-color-white-tint":J.colors.whiteTint,"--prism-color-ink":J.colors.ink,"--prism-color-text":J.colors.text,"--prism-color-text-muted":J.colors.textMuted,"--prism-color-text-soft":J.colors.textSoft,"--prism-color-text-subtle":J.colors.textSubtle,"--prism-color-placeholder":J.colors.placeholder,"--prism-color-border":J.colors.border,"--prism-color-border-strong":J.colors.borderStrong,"--prism-color-border-input":J.colors.borderInput,"--prism-color-border-faint":J.colors.borderFaint,"--prism-color-accent":J.colors.accent,"--prism-color-accent-bright":J.colors.accentBright,"--prism-color-accent-glow":J.colors.accentGlow,"--prism-color-accent-soft":J.colors.accentSoft,"--prism-color-accent-hover":J.colors.accentHover,"--prism-color-action":J.colors.action,"--prism-color-action-end":J.colors.actionEnd,"--prism-color-action-hover":J.colors.actionHover,"--prism-color-action-hover-end":J.colors.actionHoverEnd,"--prism-color-action-active":J.colors.actionActive,"--prism-color-action-active-end":J.colors.actionActiveEnd,"--prism-color-primary":J.colors.primary,"--prism-color-primary-end":J.colors.primaryEnd,"--prism-color-primary-hover":J.colors.primaryHover,"--prism-color-primary-hover-end":J.colors.primaryHoverEnd,"--prism-color-primary-active":J.colors.primaryActive,"--prism-color-primary-active-end":J.colors.primaryActiveEnd,"--prism-color-secondary":J.colors.secondary,"--prism-color-secondary-end":J.colors.secondaryEnd,"--prism-color-secondary-hover":J.colors.secondaryHover,"--prism-color-secondary-hover-end":J.colors.secondaryHoverEnd,"--prism-color-secondary-active":J.colors.secondaryActive,"--prism-color-secondary-active-end":J.colors.secondaryActiveEnd,"--prism-color-tertiary":J.colors.tertiary,"--prism-color-tertiary-end":J.colors.tertiaryEnd,"--prism-color-tertiary-hover":J.colors.tertiaryHover,"--prism-color-tertiary-hover-end":J.colors.tertiaryHoverEnd,"--prism-color-tertiary-active":J.colors.tertiaryActive,"--prism-color-tertiary-active-end":J.colors.tertiaryActiveEnd,"--prism-color-error":J.colors.error,"--prism-color-error-end":J.colors.errorEnd,"--prism-color-warning":J.colors.warning,"--prism-color-warning-end":J.colors.warningEnd,"--prism-color-information":J.colors.information,"--prism-color-information-end":J.colors.informationEnd,"--prism-color-action-preview":J.colors.actionPreview,"--prism-color-action-preview-hover":J.colors.actionPreviewHover,"--prism-color-action-glow":J.colors.actionGlow,"--prism-color-action-hover-glow":J.colors.actionHoverGlow,"--prism-color-action-active-shadow":J.colors.actionActiveShadow,"--prism-color-action-focus-glow":J.colors.actionFocusGlow,"--prism-color-focus":J.colors.focus,"--prism-color-focus-glow":J.colors.focusGlow,"--prism-color-focus-strong-glow":J.colors.focusStrongGlow,"--prism-color-invalid-glow":J.colors.invalidGlow,"--prism-color-success":J.colors.success,"--prism-color-success-bright":J.colors.successBright,"--prism-color-success-glow":J.colors.successGlow,"--prism-color-preview-glow":J.colors.previewGlow,"--prism-color-lavender-border":J.colors.lavenderBorder,"--prism-color-lavender-surface":J.colors.lavenderSurface,"--prism-color-mint-border":J.colors.mintBorder,"--prism-color-mint-surface":J.colors.mintSurface,"--prism-color-peach-border":J.colors.peachBorder,"--prism-color-peach-surface":J.colors.peachSurface,"--prism-font-size-micro":J.fontSizes.micro,"--prism-font-size-label":J.fontSizes.label,"--prism-font-size-small":J.fontSizes.small,"--prism-font-size-body-small":J.fontSizes.bodySmall,"--prism-font-size-compact":J.fontSizes.compact,"--prism-font-size-body":J.fontSizes.body,"--prism-font-size-copy":J.fontSizes.copy,"--prism-font-size-card-copy":J.fontSizes.cardCopy,"--prism-font-size-ui":J.fontSizes.ui,"--prism-font-size-lead":J.fontSizes.lead,"--prism-font-size-heading":J.fontSizes.heading,"--prism-font-size-hero":J.fontSizes.hero,"--prism-font-size-detail-hero":J.fontSizes.detailHero,"--prism-radius-control":J.radii.control,"--prism-radius-card":J.radii.card,"--prism-radius-preview":J.radii.preview,"--prism-radius-surface":J.radii.surface,"--prism-shadow-card":J.shadows.card,"--prism-shadow-action":J.shadows.action,"--prism-shadow-action-hover":J.shadows.actionHover,"--prism-shadow-action-active":J.shadows.actionActive}),er=tt(`
  :root {
    color-scheme: light;
    ${Object.entries($n).map(([e,t])=>`${e}: ${t};`).join(`
    `)}
  }

  ${Object.entries(Qn).map(([e,t])=>`
  [data-prism-palette="${e}"] {
    ${Object.entries(t).map(([e,t])=>`${e}: ${t};`).join(`
    `)}
  }`).join(`
`)}

  
  .prism-theme-model-prism {
    color-scheme: light;
  }

  .prism-theme-model-aurora {
    color-scheme: light;
    --prism-color-page: #f3f6ff;
    --prism-color-surface: #ffffff;
    --prism-color-surface-glass: rgb(255 255 255 / 78%);
    --prism-color-surface-card: rgb(255 255 255 / 86%);
    --prism-color-surface-tint: #fbf8ff;
    --prism-color-surface-raised: #ffffff;
    --prism-color-surface-hover: #fbf8ff;
    --prism-color-white: #ffffff;
    --prism-color-white-strong: rgb(255 255 255 / 92%);
    --prism-color-white-tint: rgb(255 255 255 / 30%);
    --prism-color-ink: #272344;
    --prism-color-text: #605a7b;
    --prism-color-text-muted: #70688e;
    --prism-color-text-soft: #817b9b;
    --prism-color-text-subtle: #958fad;
    --prism-color-placeholder: #b2abc5;
    --prism-color-border: #e6e0f4;
    --prism-color-border-strong: #d2c8ea;
    --prism-color-border-input: #dcd4ef;
    --prism-color-border-faint: #f0ecf7;
    --prism-color-accent: #c15fce;
    --prism-color-accent-bright: #df7be8;
    --prism-color-accent-glow: rgb(223 123 232 / 15%);
    --prism-color-accent-soft: #fff0fc;
    --prism-color-action: #6958de;
    --prism-color-action-end: #58bfc6;
    --prism-color-action-hover: #5d4dcc;
    --prism-color-action-hover-end: #43aeb5;
    --prism-color-action-active: #4e3fb5;
    --prism-color-action-active-end: #32969e;
    --prism-color-focus: #7568ed;
    --prism-color-focus-glow: rgb(117 104 237 / 18%);
    --prism-color-focus-strong-glow: rgb(117 104 237 / 25%);
    --prism-color-preview-glow: rgb(202 159 255 / 18%);
    --prism-color-lavender-border: #d8caff;
    --prism-color-lavender-surface: #f4efff;
    --prism-color-mint-border: #c6e9e2;
    --prism-color-mint-surface: #ecfbf6;
    --prism-color-peach-border: #f3d4cf;
    --prism-color-peach-surface: #fff3f0;
    --prism-radius-control: .8rem;
    --prism-radius-card: 1.45rem;
    --prism-radius-preview: 1.15rem;
    --prism-radius-surface: 1.15rem;
    --prism-shadow-card: 0 .95rem 2.6rem rgb(109 94 247 / 10%);
    --prism-shadow-action: 0 .45rem 1rem rgb(105 88 222 / 22%);
    --prism-shadow-action-hover: 0 .6rem 1.25rem rgb(105 88 222 / 28%);
    --prism-shadow-action-active: 0 .15rem .35rem rgb(105 88 222 / 18%);
    --prism-button-primary-background: linear-gradient(135deg, #6958de, #58bfc6);
    --prism-button-primary-background-hover: linear-gradient(135deg, #5d4dcc, #43aeb5);
    --prism-button-primary-background-active: linear-gradient(135deg, #4e3fb5, #32969e);
    --prism-button-primary-shadow: 0 .45rem 1rem rgb(105 88 222 / 22%);
    --prism-button-primary-shadow-hover: 0 .6rem 1.25rem rgb(105 88 222 / 28%);
    --prism-button-primary-shadow-active: 0 .15rem .35rem rgb(105 88 222 / 18%);
    --prism-button-secondary-background: linear-gradient(135deg, #7d7a9e, #9691b5);
    --prism-button-secondary-background-hover: linear-gradient(135deg, #6d6a8f, #8580a4);
    --prism-button-secondary-background-active: linear-gradient(135deg, #5f5c7d, #74708f);
    --prism-button-secondary-shadow: 0 .36rem .9rem rgb(125 122 158 / 20%);
    --prism-button-secondary-shadow-hover: 0 .52rem 1.1rem rgb(125 122 158 / 26%);
    --prism-button-secondary-shadow-active: 0 .12rem .32rem rgb(95 92 125 / 24%);
    --prism-button-tertiary-background: linear-gradient(135deg, #c15fce, #ef91c7);
    --prism-button-tertiary-background-hover: linear-gradient(135deg, #ae4dbd, #df7db6);
    --prism-button-tertiary-background-active: linear-gradient(135deg, #963da5, #c865a0);
    --prism-button-tertiary-shadow: 0 .42rem .96rem rgb(193 95 206 / 22%);
    --prism-button-tertiary-shadow-hover: 0 .56rem 1.14rem rgb(193 95 206 / 28%);
    --prism-button-tertiary-shadow-active: 0 .12rem .32rem rgb(150 61 165 / 24%);
  }

  .prism-theme-model-nocturne {
    color-scheme: dark;
    --prism-color-page: #080d1b;
    --prism-color-surface: #10172d;
    --prism-color-surface-glass: rgb(18 27 53 / 91%);
    --prism-color-surface-card: rgb(17 26 53 / 94%);
    --prism-color-surface-tint: #141f3d;
    --prism-color-surface-raised: #111a35;
    --prism-color-surface-hover: #18264a;
    --prism-color-white: #172342;
    --prism-color-white-strong: rgb(35 49 86 / 92%);
    --prism-color-white-tint: rgb(126 154 226 / 16%);
    --prism-color-ink: #ffffff;
    --prism-color-text: #b5c6e8;
    --prism-color-text-muted: #9aadd4;
    --prism-color-text-soft: #8fa5d0;
    --prism-color-text-subtle: #768bb7;
    --prism-color-placeholder: #6278a5;
    --prism-color-border: #28375e;
    --prism-color-border-strong: #3b5283;
    --prism-color-border-input: #3a4e7d;
    --prism-color-border-faint: #1d2a4a;
    --prism-color-accent: #ff8da4;
    --prism-color-accent-bright: #ff9f91;
    --prism-color-accent-glow: rgb(255 141 164 / 16%);
    --prism-color-accent-soft: #34233d;
    --prism-color-action: #7b8dff;
    --prism-color-action-end: #59c8ee;
    --prism-color-action-hover: #91a1ff;
    --prism-color-action-hover-end: #73d5f4;
    --prism-color-action-active: #6275e7;
    --prism-color-action-active-end: #43afd7;
    --prism-color-focus: #a3caff;
    --prism-color-focus-glow: rgb(126 217 255 / 22%);
    --prism-color-focus-strong-glow: rgb(126 217 255 / 32%);
    --prism-color-preview-glow: rgb(92 111 218 / 23%);
    --prism-color-lavender-border: #5360a0;
    --prism-color-lavender-surface: #202957;
    --prism-color-mint-border: #3b7f79;
    --prism-color-mint-surface: #173a3e;
    --prism-color-peach-border: #895261;
    --prism-color-peach-surface: #3a2539;
    --prism-radius-control: .72rem;
    --prism-radius-card: 1.35rem;
    --prism-radius-preview: 1.1rem;
    --prism-radius-surface: 1rem;
    --prism-shadow-card: 0 .95rem 2.6rem rgb(0 0 0 / 28%);
    --prism-shadow-action: 0 .45rem 1rem rgb(68 104 221 / 30%);
    --prism-shadow-action-hover: 0 .6rem 1.25rem rgb(68 104 221 / 40%);
    --prism-shadow-action-active: 0 .15rem .35rem rgb(0 0 0 / 30%);
    --prism-button-primary-background: linear-gradient(135deg, #697cff, #4ab8df);
    --prism-button-primary-background-hover: linear-gradient(135deg, #8292ff, #68c7e8);
    --prism-button-primary-background-active: linear-gradient(135deg, #5669dc, #3aa3ca);
    --prism-button-primary-shadow: 0 .45rem 1rem rgb(68 104 221 / 30%);
    --prism-button-primary-shadow-hover: 0 .6rem 1.25rem rgb(68 104 221 / 40%);
    --prism-button-primary-shadow-active: 0 .15rem .35rem rgb(0 0 0 / 30%);
    --prism-button-secondary-background: linear-gradient(135deg, #465779, #6278a4);
    --prism-button-secondary-background-hover: linear-gradient(135deg, #53678d, #7188b5);
    --prism-button-secondary-background-active: linear-gradient(135deg, #394967, #52678f);
    --prism-button-secondary-shadow: 0 .36rem .9rem rgb(0 0 0 / 24%);
    --prism-button-secondary-shadow-hover: 0 .52rem 1.1rem rgb(0 0 0 / 32%);
    --prism-button-secondary-shadow-active: 0 .12rem .32rem rgb(0 0 0 / 30%);
    --prism-button-tertiary-background: linear-gradient(135deg, #e66f91, #f3a276);
    --prism-button-tertiary-background-hover: linear-gradient(135deg, #f381a0, #ffb188);
    --prism-button-tertiary-background-active: linear-gradient(135deg, #c95679, #d78361);
    --prism-button-tertiary-shadow: 0 .42rem .96rem rgb(230 111 145 / 24%);
    --prism-button-tertiary-shadow-hover: 0 .56rem 1.14rem rgb(230 111 145 / 34%);
    --prism-button-tertiary-shadow-active: 0 .12rem .32rem rgb(0 0 0 / 28%);
    --prism-button-focus-outline: 3px solid rgb(126 217 255 / 70%);
  }

  .prism-theme-model-editorial {
    color-scheme: light;
    font-family: Georgia, 'Times New Roman', serif;
    --prism-color-page: #f7f3ed;
    --prism-color-surface: #fffdf9;
    --prism-color-surface-glass: rgb(255 253 249 / 88%);
    --prism-color-surface-card: rgb(255 253 249 / 94%);
    --prism-color-surface-tint: #f3eee7;
    --prism-color-surface-raised: #fffdf9;
    --prism-color-surface-hover: #f3eee7;
    --prism-color-white: #fffdf9;
    --prism-color-white-strong: rgb(255 253 249 / 94%);
    --prism-color-white-tint: rgb(255 253 249 / 32%);
    --prism-color-ink: #2e2926;
    --prism-color-text: #615b56;
    --prism-color-text-muted: #746c65;
    --prism-color-text-soft: #817870;
    --prism-color-text-subtle: #9b9188;
    --prism-color-placeholder: #b6aaa0;
    --prism-color-border: #e5ddd4;
    --prism-color-border-strong: #d0c4b8;
    --prism-color-border-input: #d9cec3;
    --prism-color-border-faint: #eee8e1;
    --prism-color-accent: #c75a3e;
    --prism-color-accent-bright: #df7654;
    --prism-color-accent-glow: rgb(199 90 62 / 15%);
    --prism-color-accent-soft: #fff0e9;
    --prism-color-action: #2e6878;
    --prism-color-action-end: #4d8b8c;
    --prism-color-action-hover: #245666;
    --prism-color-action-hover-end: #3d797a;
    --prism-color-action-active: #1c4654;
    --prism-color-action-active-end: #306567;
    --prism-color-focus: #3b7c89;
    --prism-color-focus-glow: rgb(59 124 137 / 18%);
    --prism-color-focus-strong-glow: rgb(59 124 137 / 25%);
    --prism-color-preview-glow: rgb(229 151 119 / 17%);
    --prism-color-lavender-border: #d5c9df;
    --prism-color-lavender-surface: #f3edf7;
    --prism-color-mint-border: #c7ddd5;
    --prism-color-mint-surface: #edf7f2;
    --prism-color-peach-border: #eac5b8;
    --prism-color-peach-surface: #fff0e8;
    --prism-radius-control: .35rem;
    --prism-radius-card: .9rem;
    --prism-radius-preview: .55rem;
    --prism-radius-surface: .65rem;
    --prism-shadow-card: 0 .8rem 2rem rgb(74 58 45 / 8%);
    --prism-shadow-action: 0 .35rem .8rem rgb(46 104 120 / 20%);
    --prism-shadow-action-hover: 0 .5rem 1rem rgb(46 104 120 / 26%);
    --prism-shadow-action-active: 0 .1rem .25rem rgb(28 70 84 / 18%);
    --prism-button-primary-background: linear-gradient(135deg, #2e6878, #4d8b8c);
    --prism-button-primary-background-hover: linear-gradient(135deg, #245666, #3d797a);
    --prism-button-primary-background-active: linear-gradient(135deg, #1c4654, #306567);
    --prism-button-primary-shadow: 0 .35rem .8rem rgb(46 104 120 / 20%);
    --prism-button-primary-shadow-hover: 0 .5rem 1rem rgb(46 104 120 / 26%);
    --prism-button-primary-shadow-active: 0 .1rem .25rem rgb(28 70 84 / 18%);
    --prism-button-secondary-background: linear-gradient(135deg, #817870, #9a8e84);
    --prism-button-secondary-background-hover: linear-gradient(135deg, #71675f, #897c71);
    --prism-button-secondary-background-active: linear-gradient(135deg, #625950, #786b60);
    --prism-button-secondary-shadow: 0 .32rem .75rem rgb(97 91 86 / 16%);
    --prism-button-secondary-shadow-hover: 0 .48rem .95rem rgb(97 91 86 / 22%);
    --prism-button-secondary-shadow-active: 0 .1rem .25rem rgb(80 70 60 / 18%);
    --prism-button-tertiary-background: linear-gradient(135deg, #c75a3e, #df7654);
    --prism-button-tertiary-background-hover: linear-gradient(135deg, #b44a30, #d36445);
    --prism-button-tertiary-background-active: linear-gradient(135deg, #983c28, #b95038);
    --prism-button-tertiary-shadow: 0 .35rem .85rem rgb(199 90 62 / 20%);
    --prism-button-tertiary-shadow-hover: 0 .5rem 1rem rgb(199 90 62 / 26%);
    --prism-button-tertiary-shadow-active: 0 .1rem .25rem rgb(152 60 40 / 18%);
  }

  .prism-theme-model-terminal {
    color-scheme: dark;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    --prism-color-page: #07100e;
    --prism-color-surface: #0d1815;
    --prism-color-surface-glass: rgb(13 24 21 / 93%);
    --prism-color-surface-card: rgb(13 24 21 / 96%);
    --prism-color-surface-tint: #10221c;
    --prism-color-surface-raised: #0d1815;
    --prism-color-surface-hover: #10221c;
    --prism-color-white: #12251d;
    --prism-color-white-strong: rgb(28 59 45 / 94%);
    --prism-color-white-tint: rgb(91 224 148 / 15%);
    --prism-color-ink: #d8ffe4;
    --prism-color-text: #a7d8b6;
    --prism-color-text-muted: #8fc29e;
    --prism-color-text-soft: #7cae8c;
    --prism-color-text-subtle: #638e72;
    --prism-color-placeholder: #4d765d;
    --prism-color-border: #234b38;
    --prism-color-border-strong: #32674c;
    --prism-color-border-input: #2c5c43;
    --prism-color-border-faint: #173326;
    --prism-color-accent: #ffbd72;
    --prism-color-accent-bright: #ffd18e;
    --prism-color-accent-glow: rgb(255 189 114 / 15%);
    --prism-color-accent-soft: #34271b;
    --prism-color-action: #45d483;
    --prism-color-action-end: #92e66d;
    --prism-color-action-hover: #66e39a;
    --prism-color-action-hover-end: #aceb87;
    --prism-color-action-active: #2dbd6e;
    --prism-color-action-active-end: #75d253;
    --prism-color-focus: #79eca4;
    --prism-color-focus-glow: rgb(91 224 148 / 20%);
    --prism-color-focus-strong-glow: rgb(91 224 148 / 30%);
    --prism-color-preview-glow: rgb(62 190 116 / 17%);
    --prism-color-lavender-border: #35664e;
    --prism-color-lavender-surface: #163126;
    --prism-color-mint-border: #387256;
    --prism-color-mint-surface: #143326;
    --prism-color-peach-border: #795636;
    --prism-color-peach-surface: #30251a;
    --prism-radius-control: .35rem;
    --prism-radius-card: .75rem;
    --prism-radius-preview: .55rem;
    --prism-radius-surface: .6rem;
    --prism-shadow-card: 0 .8rem 2rem rgb(0 0 0 / 28%);
    --prism-shadow-action: 0 .35rem .8rem rgb(45 189 110 / 20%);
    --prism-shadow-action-hover: 0 .5rem 1rem rgb(45 189 110 / 30%);
    --prism-shadow-action-active: 0 .1rem .25rem rgb(0 0 0 / 30%);
    --prism-button-primary-background: linear-gradient(135deg, #35c879, #8cdf68);
    --prism-button-primary-background-hover: linear-gradient(135deg, #50dc8b, #a2eb82);
    --prism-button-primary-background-active: linear-gradient(135deg, #26ad65, #70c94f);
    --prism-button-primary-shadow: 0 .35rem .8rem rgb(45 189 110 / 20%);
    --prism-button-primary-shadow-hover: 0 .5rem 1rem rgb(45 189 110 / 30%);
    --prism-button-primary-shadow-active: 0 .1rem .25rem rgb(0 0 0 / 30%);
    --prism-button-secondary-background: linear-gradient(135deg, #315a47, #47765a);
    --prism-button-secondary-background-hover: linear-gradient(135deg, #3c6d53, #578866);
    --prism-button-secondary-background-active: linear-gradient(135deg, #274a3a, #3c644d);
    --prism-button-secondary-shadow: 0 .32rem .75rem rgb(0 0 0 / 24%);
    --prism-button-secondary-shadow-hover: 0 .48rem .95rem rgb(0 0 0 / 32%);
    --prism-button-secondary-shadow-active: 0 .1rem .25rem rgb(0 0 0 / 30%);
    --prism-button-tertiary-background: linear-gradient(135deg, #c9794a, #e6a45d);
    --prism-button-tertiary-background-hover: linear-gradient(135deg, #dc8956, #f0b36d);
    --prism-button-tertiary-background-active: linear-gradient(135deg, #a96039, #c98949);
    --prism-button-tertiary-shadow: 0 .35rem .85rem rgb(201 121 74 / 20%);
    --prism-button-tertiary-shadow-hover: 0 .5rem 1rem rgb(201 121 74 / 28%);
    --prism-button-tertiary-shadow-active: 0 .1rem .25rem rgb(0 0 0 / 28%);
    --prism-button-focus-outline: 3px solid rgb(91 224 148 / 70%);
  }


  .prism-icon {
    display: block;
    flex: 0 0 auto;
  }

  .prism-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: .45rem;
    padding: var(--prism-button-padding);
    border: var(--prism-button-border-width) solid var(--prism-button-current-border);
    border-radius: var(--prism-button-radius);
    color: var(--prism-button-current-text);
    background: var(--prism-button-current-background);
    font-size: var(--prism-button-font-size);
    font-weight: var(--prism-button-font-weight);
    line-height: 1.1;
    cursor: pointer;
    box-shadow: var(--prism-button-current-shadow);
    appearance: none;
    transform: translateY(0);
    transition: background .18s ease, box-shadow .18s ease, transform .18s ease, opacity .18s ease, border-color .18s ease;
  }

  .prism-button-small {
    min-height: 2rem;
    gap: .35rem;
    padding: .42rem .62rem;
    font-size: var(--prism-font-size-micro);
  }

  .prism-button-medium {
    min-height: 2.4rem;
  }

  .prism-button-large {
    min-height: 3rem;
    gap: .58rem;
    padding: .78rem 1.08rem;
    font-size: var(--prism-font-size-body);
  }

  .prism-button-pill {
    border-radius: 999px;
  }

  .prism-button-square {
    border-radius: .48rem;
  }

  .prism-button-full-width {
    width: 100%;
  }

  .prism-button-icon-only {
    width: 2.4rem;
    min-width: 2.4rem;
    padding: 0;
    aspect-ratio: 1;
  }

  .prism-button-small.prism-button-icon-only {
    width: 2rem;
    min-width: 2rem;
  }

  .prism-button-large.prism-button-icon-only {
    width: 3rem;
    min-width: 3rem;
  }

  .prism-button-full-width.prism-button-icon-only {
    width: 100%;
    aspect-ratio: auto;
  }

  .prism-button-icon {
    display: inline-grid;
    width: 1em;
    height: 1em;
    flex: 0 0 1em;
    place-items: center;
    font-size: 1.08em;
  }

  .prism-button-icon > svg {
    width: 100%;
    height: 100%;
  }

  .prism-button-label {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .prism-button-spinner {
    width: .9em;
    height: .9em;
    border: 1.7px solid currentColor;
    border-right-color: transparent;
    border-radius: 50%;
    animation: prism-button-spin .72s linear infinite;
  }

  .prism-button-pressed {
    background: var(--prism-button-current-background-active);
    box-shadow: var(--prism-button-current-shadow-active);
    transform: translateY(1px);
  }

  @keyframes prism-button-spin {
    to {
      transform: rotate(1turn);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .prism-button-spinner {
      animation-duration: 1.8s;
    }
  }

  .prism-button-primary {
    --prism-button-current-text: var(--prism-button-primary-text);
    --prism-button-current-border: var(--prism-button-primary-border);
    --prism-button-current-background: var(--prism-button-primary-background);
    --prism-button-current-background-hover: var(--prism-button-primary-background-hover);
    --prism-button-current-background-active: var(--prism-button-primary-background-active);
    --prism-button-current-shadow: var(--prism-button-primary-shadow);
    --prism-button-current-shadow-hover: var(--prism-button-primary-shadow-hover);
    --prism-button-current-shadow-active: var(--prism-button-primary-shadow-active);
  }

  .prism-button-secondary {
    --prism-button-current-text: var(--prism-button-secondary-text);
    --prism-button-current-border: var(--prism-button-secondary-border);
    --prism-button-current-background: var(--prism-button-secondary-background);
    --prism-button-current-background-hover: var(--prism-button-secondary-background-hover);
    --prism-button-current-background-active: var(--prism-button-secondary-background-active);
    --prism-button-current-shadow: var(--prism-button-secondary-shadow);
    --prism-button-current-shadow-hover: var(--prism-button-secondary-shadow-hover);
    --prism-button-current-shadow-active: var(--prism-button-secondary-shadow-active);
  }

  .prism-button-tertiary {
    --prism-button-current-text: var(--prism-button-tertiary-text);
    --prism-button-current-border: var(--prism-button-tertiary-border);
    --prism-button-current-background: var(--prism-button-tertiary-background);
    --prism-button-current-background-hover: var(--prism-button-tertiary-background-hover);
    --prism-button-current-background-active: var(--prism-button-tertiary-background-active);
    --prism-button-current-shadow: var(--prism-button-tertiary-shadow);
    --prism-button-current-shadow-hover: var(--prism-button-tertiary-shadow-hover);
    --prism-button-current-shadow-active: var(--prism-button-tertiary-shadow-active);
  }

  .prism-button-error {
    --prism-button-current-text: var(--prism-button-error-text);
    --prism-button-current-border: var(--prism-button-error-border);
    --prism-button-current-background: var(--prism-button-error-background);
    --prism-button-current-background-hover: var(--prism-button-error-background-hover);
    --prism-button-current-background-active: var(--prism-button-error-background-active);
    --prism-button-current-shadow: var(--prism-button-error-shadow);
    --prism-button-current-shadow-hover: var(--prism-button-error-shadow-hover);
    --prism-button-current-shadow-active: var(--prism-button-error-shadow-active);
  }

  .prism-button-warning {
    --prism-button-current-text: var(--prism-button-warning-text);
    --prism-button-current-border: var(--prism-button-warning-border);
    --prism-button-current-background: var(--prism-button-warning-background);
    --prism-button-current-background-hover: var(--prism-button-warning-background-hover);
    --prism-button-current-background-active: var(--prism-button-warning-background-active);
    --prism-button-current-shadow: var(--prism-button-warning-shadow);
    --prism-button-current-shadow-hover: var(--prism-button-warning-shadow-hover);
    --prism-button-current-shadow-active: var(--prism-button-warning-shadow-active);
  }

  .prism-button-information {
    --prism-button-current-text: var(--prism-button-information-text);
    --prism-button-current-border: var(--prism-button-information-border);
    --prism-button-current-background: var(--prism-button-information-background);
    --prism-button-current-background-hover: var(--prism-button-information-background-hover);
    --prism-button-current-background-active: var(--prism-button-information-background-active);
    --prism-button-current-shadow: var(--prism-button-information-shadow);
    --prism-button-current-shadow-hover: var(--prism-button-information-shadow-hover);
    --prism-button-current-shadow-active: var(--prism-button-information-shadow-active);
  }

  .prism-button-success {
    --prism-button-current-text: var(--prism-button-success-text);
    --prism-button-current-border: var(--prism-button-success-border);
    --prism-button-current-background: var(--prism-button-success-background);
    --prism-button-current-background-hover: var(--prism-button-success-background-hover);
    --prism-button-current-background-active: var(--prism-button-success-background-active);
    --prism-button-current-shadow: var(--prism-button-success-shadow);
    --prism-button-current-shadow-hover: var(--prism-button-success-shadow-hover);
    --prism-button-current-shadow-active: var(--prism-button-success-shadow-active);
  }

  .prism-button:hover:not(:disabled) {
    background: var(--prism-button-current-background-hover);
    box-shadow: var(--prism-button-current-shadow-hover);
    transform: var(--prism-button-transform-hover);
  }

  .prism-button-pressed:hover:not(:disabled) {
    background: var(--prism-button-current-background-active);
    box-shadow: var(--prism-button-current-shadow-active);
    transform: translateY(1px);
  }

  .prism-button:active:not(:disabled) {
    background: var(--prism-button-current-background-active);
    box-shadow: var(--prism-button-current-shadow-active);
    transform: var(--prism-button-transform-active);
  }

  .prism-button:focus-visible {
    outline: var(--prism-button-focus-outline);
    outline-offset: var(--prism-button-focus-offset);
  }

  .prism-button:disabled {
    opacity: var(--prism-button-disabled-opacity);
    cursor: not-allowed;
  }

  .prism-select {
    position: relative;
    display: block;
    width: 100%;
  }

  .prism-select-trigger {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: .8rem;
    width: 100%;
    padding: .72rem .8rem;
    border: 1px solid var(--prism-color-border-input);
    border-radius: var(--prism-radius-control);
    outline: none;
    color: var(--prism-color-ink);
    background: var(--prism-color-surface-raised);
    font: inherit;
    font-size: var(--prism-font-size-body);
    line-height: 1.2;
    text-align: left;
    cursor: pointer;
    appearance: none;
    transition: border-color .18s ease, box-shadow .18s ease, background .18s ease;
  }

  .prism-select-trigger:hover:not(:disabled) {
    border-color: var(--prism-color-border-strong);
    background: var(--prism-color-surface-tint);
  }

  .prism-select-trigger:focus-visible {
    border-color: var(--prism-color-focus);
    box-shadow: 0 0 0 .25rem var(--prism-color-focus-glow);
  }

  .prism-select-trigger:disabled {
    opacity: var(--prism-button-disabled-opacity);
    cursor: not-allowed;
  }

  .prism-select-chevron {
    width: .55rem;
    height: .55rem;
    flex: 0 0 .55rem;
    border-right: 1.5px solid currentColor;
    border-bottom: 1.5px solid currentColor;
    transform: translateY(-.15rem) rotate(45deg);
    transition: transform .18s ease;
  }

  .prism-select-trigger[aria-expanded="true"] .prism-select-chevron {
    transform: translateY(.15rem) rotate(225deg);
  }

  .prism-select-menu {
    position: absolute;
    z-index: 20;
    display: grid;
    gap: .2rem;
    min-width: 100%;
    max-width: min(20rem, calc(100vw - 1rem));
    max-height: min(18rem, calc(100vh - 1rem));
    padding: .35rem;
    overflow: auto;
    border: 1px solid var(--prism-color-border-input);
    border-radius: var(--prism-radius-control);
    background: var(--prism-color-surface-raised);
    box-shadow: 0 .7rem 1.8rem rgb(37 49 78 / 15%), 0 .1rem .3rem rgb(37 49 78 / 8%);
  }

  .prism-select-menu[hidden] {
    display: none;
  }

  .prism-select-menu-bottom {
    top: calc(100% + .35rem);
    right: 0;
    left: 0;
  }

  .prism-select-menu-top {
    right: 0;
    bottom: calc(100% + .35rem);
    left: 0;
  }

  .prism-select-menu-right {
    top: 0;
    left: calc(100% + .35rem);
  }

  .prism-select-menu-left {
    top: 0;
    right: calc(100% + .35rem);
  }

  .prism-select-option {
    display: block;
    width: 100%;
    padding: .62rem .7rem;
    border: 0;
    border-radius: calc(var(--prism-radius-control) - .2rem);
    color: var(--prism-color-text);
    background: transparent;
    font: inherit;
    font-size: var(--prism-font-size-body);
    line-height: 1.25;
    text-align: left;
    cursor: pointer;
  }

  .prism-select-option:hover:not(:disabled),
  .prism-select-option[aria-selected="true"],
  .prism-select-option[data-active="true"] {
    color: var(--prism-color-ink);
    background: var(--prism-color-lavender-surface);
  }

  .prism-select-option:focus-visible {
    outline: 2px solid var(--prism-color-focus);
    outline-offset: -2px;
  }

  .prism-select-option:disabled {
    opacity: .48;
    cursor: not-allowed;
  }

  .prism-select-small .prism-select-trigger {
    padding: .56rem .68rem;
    font-size: var(--prism-font-size-small);
  }

  .prism-select-large .prism-select-trigger {
    padding: .86rem .95rem;
    font-size: var(--prism-font-size-copy);
  }

  .prism-auto-complete {
    position: relative;
    display: grid;
    width: 100%;
  }

  .prism-auto-complete-label {
    display: block;
    margin-bottom: .4rem;
    color: var(--prism-color-text-strong);
    font-size: var(--prism-font-size-small);
    font-weight: 750;
  }

  .prism-auto-complete-control {
    position: relative;
  }

  .prism-auto-complete-input {
    width: 100%;
    min-height: 2.65rem;
    padding: .65rem 2.4rem .65rem .8rem;
    border: 1px solid var(--prism-color-border-input);
    border-radius: var(--prism-radius-control);
    outline: none;
    color: var(--prism-color-ink);
    background: var(--prism-color-surface-raised);
    font: inherit;
    font-size: var(--prism-font-size-body);
    line-height: 1.2;
    appearance: none;
    transition: border-color .18s ease, box-shadow .18s ease, background .18s ease;
  }

  .prism-auto-complete-input::placeholder {
    color: var(--prism-color-placeholder);
  }

  .prism-auto-complete-input:hover:not(:disabled) {
    border-color: var(--prism-color-border-strong);
    background: var(--prism-color-surface-tint);
  }

  .prism-auto-complete-input:focus-visible {
    border-color: var(--prism-color-focus);
    box-shadow: 0 0 0 .25rem var(--prism-color-focus-glow);
  }

  .prism-auto-complete-input:disabled {
    opacity: var(--prism-button-disabled-opacity);
    cursor: not-allowed;
  }

  .prism-auto-complete-input-invalid {
    border-color: var(--prism-color-error);
  }

  .prism-auto-complete-chevron {
    position: absolute;
    top: 50%;
    right: .9rem;
    width: .55rem;
    height: .55rem;
    border-right: 1.5px solid var(--prism-color-text-muted);
    border-bottom: 1.5px solid var(--prism-color-text-muted);
    pointer-events: none;
    transform: translateY(-70%) rotate(45deg);
    transition: transform .18s ease, border-color .18s ease;
  }

  .prism-auto-complete-input[aria-expanded="true"] + .prism-auto-complete-chevron {
    border-color: var(--prism-color-focus);
    transform: translateY(-25%) rotate(225deg);
  }

  .prism-auto-complete-menu {
    position: absolute;
    z-index: 20;
    right: 0;
    left: 0;
    display: grid;
    gap: .2rem;
    max-height: min(18rem, calc(100vh - 1rem));
    padding: .35rem;
    overflow: auto;
    border: 1px solid var(--prism-color-border-input);
    border-radius: var(--prism-radius-control);
    background: var(--prism-color-surface-raised);
    box-shadow: 0 .7rem 1.8rem rgb(37 49 78 / 15%), 0 .1rem .3rem rgb(37 49 78 / 8%);
  }

  .prism-auto-complete-menu[hidden] {
    display: none;
  }

  .prism-auto-complete-menu-bottom {
    top: calc(100% + .35rem);
  }

  .prism-auto-complete-menu-top {
    bottom: calc(100% + .35rem);
  }

  .prism-auto-complete-option {
    display: block;
    width: 100%;
    padding: .62rem .7rem;
    border: 0;
    border-radius: calc(var(--prism-radius-control) - .2rem);
    color: var(--prism-color-text);
    background: transparent;
    font: inherit;
    font-size: var(--prism-font-size-body);
    line-height: 1.25;
    text-align: left;
    cursor: pointer;
  }

  .prism-auto-complete-option:hover:not(:disabled),
  .prism-auto-complete-option-active,
  .prism-auto-complete-option-selected {
    color: var(--prism-color-ink);
    background: var(--prism-color-lavender-surface);
  }

  .prism-auto-complete-option:focus-visible {
    outline: 2px solid var(--prism-color-focus);
    outline-offset: -2px;
  }

  .prism-auto-complete-option:disabled {
    opacity: .48;
    cursor: not-allowed;
  }

  .prism-auto-complete-status {
    padding: .7rem;
    color: var(--prism-color-text-muted);
    font-size: var(--prism-font-size-small);
  }

  .prism-auto-complete-message {
    display: block;
    margin-top: .35rem;
    color: var(--prism-color-text-subtle);
    font-size: var(--prism-font-size-small);
  }

  .prism-auto-complete-message-error {
    color: var(--prism-color-error);
  }

  .prism-auto-complete-small .prism-auto-complete-input {
    min-height: 2.25rem;
    padding: .55rem 2.2rem .55rem .68rem;
    font-size: var(--prism-font-size-small);
  }

  .prism-auto-complete-large .prism-auto-complete-input {
    min-height: 3rem;
    padding: .82rem 2.55rem .82rem .95rem;
    font-size: var(--prism-font-size-copy);
  }

  .text-field-message,
  .check-box-message,
  .prism-select-message {
    display: block;
    margin-top: .35rem;
    color: var(--prism-color-text-subtle);
    font-size: var(--prism-font-size-small);
  }

  .text-field {
    display: block;
    box-sizing: border-box;
    width: 100%;
    min-height: 2.65rem;
    padding: .65rem .8rem;
    border: 1px solid var(--prism-color-border-input);
    border-radius: var(--prism-radius-control);
    outline: none;
    color: var(--prism-color-ink);
    background: var(--prism-color-surface-raised);
    font: inherit;
    font-size: var(--prism-font-size-body);
    line-height: 1.2;
    appearance: none;
    transition: border-color .18s ease, box-shadow .18s ease, background .18s ease;
  }

  .text-field::placeholder {
    color: var(--prism-color-placeholder);
  }

  .text-field:hover:not(:disabled) {
    border-color: var(--prism-color-border-strong);
    background: var(--prism-color-surface-hover);
  }

  .text-field:focus-visible {
    border-color: var(--prism-color-focus);
    box-shadow: 0 0 0 .25rem var(--prism-color-focus-glow);
  }

  .text-field:disabled {
    opacity: var(--prism-button-disabled-opacity);
    cursor: not-allowed;
  }

  .text-field-small {
    min-height: 2.25rem;
    padding: .55rem .68rem;
    font-size: var(--prism-font-size-small);
  }

  .text-field-large {
    min-height: 3rem;
    padding: .82rem .95rem;
    font-size: var(--prism-font-size-copy);
  }

  .check-box {
    display: inline-grid;
    grid-template-columns: auto minmax(0, 1fr);
    align-items: start;
    gap: .55rem;
    color: var(--prism-color-text);
    cursor: pointer;
  }

  .check-box-input {
    width: 1.05rem;
    height: 1.05rem;
    margin: .08rem 0 0;
    accent-color: var(--prism-color-action);
  }

  .check-box-input:focus-visible {
    outline: 2px solid var(--prism-color-focus);
    outline-offset: 2px;
  }

  .check-box-invalid {
    color: var(--prism-color-error);
  }

  .check-box-invalid .check-box-input {
    accent-color: var(--prism-color-error);
  }

  .check-box:has(.check-box-input:disabled) {
    opacity: var(--prism-button-disabled-opacity);
    cursor: not-allowed;
  }

  .text-field-message-error,
  .check-box-message-error,
  .prism-select-message-error {
    color: var(--prism-color-error);
  }

  .text-field-invalid {
    border-color: var(--prism-color-error);
  }

  .prism-select-trigger[aria-invalid="true"] {
    border-color: var(--prism-color-error);
  }

  .prism-code-viewer {
    --prism-code-background: #111a32;
    --prism-code-background-raised: #172344;
    --prism-code-foreground: #d9e4ff;
    --prism-code-gutter: #7081a8;
    --prism-code-border: rgb(139 169 255 / 25%);
    --prism-code-keyword: #a8b5ff;
    --prism-code-string: #9ee4bf;
    --prism-code-number: #ffbd72;
    --prism-code-comment: #7182a8;
    --prism-code-function: #8bd9ff;
    --prism-code-tag: #ff9db2;
    --prism-code-tag-name: #a8b5ff;
    --prism-code-attribute: #ffd27c;
    --prism-code-property: #c1a8ff;
    --prism-code-boolean: #ffad8d;
    --prism-code-operator: #90aaff;
    --prism-code-punctuation: #9aa9c8;
    --prism-code-font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, 'Liberation Mono', monospace;
    --prism-code-font-size: .78rem;
    --prism-code-line-height: 1.7;
    --prism-code-tab-size: 2;
    --prism-code-min-height: 18rem;
    --prism-code-max-height: 32rem;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    border: 1px solid var(--prism-code-border);
    border-radius: var(--prism-radius-surface);
    color: var(--prism-code-foreground);
    background: var(--prism-code-background);
    box-shadow: 0 .8rem 1.8rem rgb(3 8 25 / 18%);
  }

  .prism-code-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: .65rem;
    min-height: 2.65rem;
    padding: .5rem .65rem .5rem .85rem;
    border-bottom: 1px solid var(--prism-code-border);
    background: var(--prism-code-background-raised);
  }

  .prism-code-file {
    display: inline-flex;
    align-items: center;
    gap: .48rem;
    min-width: 0;
    flex: 1;
    color: var(--prism-code-foreground);
    font-family: var(--prism-code-font-family);
    font-size: .72rem;
    font-weight: 700;
  }

  .prism-code-file-dot {
    width: .45rem;
    height: .45rem;
    flex: 0 0 .45rem;
    border-radius: 50%;
    background: var(--prism-color-accent-bright);
    box-shadow: 0 0 0 .2rem var(--prism-color-accent-glow);
  }

  .prism-code-filename {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .prism-code-language {
    padding: .18rem .4rem;
    border: 1px solid var(--prism-code-border);
    border-radius: 999px;
    color: var(--prism-code-gutter);
    font-size: .6rem;
    font-weight: 800;
    letter-spacing: .08em;
    text-transform: uppercase;
  }

  .prism-code-tabs {
    display: inline-flex;
    flex: 0 0 auto;
    gap: .12rem;
    padding: .14rem;
    border: 1px solid var(--prism-code-border);
    border-radius: 999px;
    background: rgb(3 8 25 / 28%);
  }

  .prism-code-tab {
    appearance: none;
    min-height: 1.55rem;
    padding: .18rem .7rem;
    border: 0;
    border-radius: 999px;
    color: var(--prism-code-gutter);
    background: transparent;
    font-family: inherit;
    font-size: .62rem;
    font-weight: 800;
    letter-spacing: .08em;
    text-transform: uppercase;
    cursor: pointer;
    transition: color .18s ease, background .18s ease, box-shadow .18s ease;
  }

  .prism-code-tab:hover {
    color: var(--prism-code-foreground);
  }

  .prism-code-tab:focus-visible {
    outline: 2px solid var(--prism-color-focus);
    outline-offset: 2px;
  }

  .prism-code-tab[aria-selected="true"] {
    color: var(--prism-code-foreground);
    background: var(--prism-code-background);
    box-shadow: 0 .2rem .6rem rgb(3 8 25 / 28%);
  }

  .prism-code-copy {
    display: inline-grid;
    width: 1.85rem;
    height: 1.85rem;
    margin-left: auto;
    place-items: center;
    padding: 0;
    border: 1px solid transparent;
    border-radius: .48rem;
    color: var(--prism-code-gutter);
    background: transparent;
    cursor: pointer;
    transition: color .18s ease, border-color .18s ease, background .18s ease, transform .18s ease;
  }

  .prism-code-status {
    position: absolute;
    width: 1px;
    height: 1px;
    margin: -1px;
    padding: 0;
    overflow: hidden;
    clip: rect(0 0 0 0);
    white-space: nowrap;
    border: 0;
  }

  .prism-code-copy:hover {
    border-color: var(--prism-code-border);
    color: var(--prism-code-foreground);
    background: rgb(255 255 255 / 8%);
    transform: translateY(-1px);
  }

  .prism-code-copy:focus-visible {
    outline: 2px solid var(--prism-color-focus);
    outline-offset: 2px;
  }

  .prism-code-viewer[data-copy-state="copied"] .prism-code-copy {
    color: var(--prism-color-success-bright);
  }

  .prism-code-viewer[data-copy-state="error"] .prism-code-copy {
    color: var(--prism-color-accent-bright);
  }

  .prism-code-body {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    flex: 1;
    min-height: var(--prism-code-min-height);
    height: 24rem;
    max-height: var(--prism-code-max-height);
    overflow: hidden;
  }

  .prism-code-gutter {
    min-width: 3.1rem;
    height: 100%;
    padding: 1.05rem .8rem 1.05rem .6rem;
    overflow: hidden;
    border-right: 1px solid var(--prism-code-border);
    color: var(--prism-code-gutter);
    font-family: var(--prism-code-font-family);
    font-size: var(--prism-code-font-size);
    line-height: var(--prism-code-line-height);
    text-align: right;
    user-select: none;
  }

  .prism-code-gutter-hidden {
    display: none;
  }

  .prism-code-gutter-lines {
    display: grid;
    align-content: start;
    transform: translateY(0);
  }

  .prism-code-gutter-line {
    display: block;
    line-height: var(--prism-code-line-height);
  }

  .prism-code-scroll {
    position: relative;
    min-width: 0;
    min-height: 0;
    height: 100%;
    overflow: hidden;
    background: var(--prism-code-background);
  }

  .prism-code-highlight,
  .prism-code-input {
    box-sizing: border-box;
    margin: 0;
    padding: 1.05rem 1.1rem;
    font-family: var(--prism-code-font-family);
    font-size: var(--prism-code-font-size);
    font-variant-ligatures: contextual;
    line-height: var(--prism-code-line-height);
    tab-size: var(--prism-code-tab-size);
    white-space: pre;
  }

  .prism-code-highlight {
    position: absolute;
    z-index: 0;
    top: 0;
    left: 0;
    width: max-content;
    min-width: 100%;
    height: max-content;
    min-height: 100%;
    color: var(--prism-code-foreground);
    pointer-events: none;
    transform: translate(0, 0);
  }

  .prism-code-highlight code {
    font: inherit;
  }

  .prism-code-input {
    position: absolute;
    z-index: 1;
    top: 0;
    right: 0;
    bottom: 0;
    left: 0;
    display: block;
    width: 100%;
    height: 100%;
    max-height: 100%;
    min-height: 0;
    overflow: auto;
    border: 0 !important;
    outline: none;
    color: transparent;
    background: transparent !important;
    caret-color: var(--prism-code-foreground);
    resize: none;
    appearance: none;
    box-shadow: none !important;
    -webkit-text-fill-color: transparent;
  }

  .prism-code-input::selection {
    color: transparent;
    background: rgb(126 217 255 / 24%);
  }

  .prism-code-token-keyword {
    color: var(--prism-code-keyword);
  }

  .prism-code-token-string {
    color: var(--prism-code-string);
  }

  .prism-code-token-number {
    color: var(--prism-code-number);
  }

  .prism-code-token-comment {
    color: var(--prism-code-comment);
    font-style: italic;
  }

  .prism-code-token-function {
    color: var(--prism-code-function);
  }

  .prism-code-token-tag,
  .prism-code-token-tag-name {
    color: var(--prism-code-tag);
  }

  .prism-code-token-tag-name {
    color: var(--prism-code-tag-name);
  }

  .prism-code-token-attribute {
    color: var(--prism-code-attribute);
  }

  .prism-code-token-property {
    color: var(--prism-code-property);
  }

  .prism-code-token-boolean {
    color: var(--prism-code-boolean);
  }

  .prism-code-token-operator {
    color: var(--prism-code-operator);
  }

  .prism-code-token-punctuation {
    color: var(--prism-code-punctuation);
  }

  .prism-layout {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr) auto;
    min-width: 0;
    min-height: 100%;
  }

  .prism-layout-body {
    display: grid;
    grid-template-columns: minmax(14rem, 18rem) minmax(0, 1fr);
    gap: 1.25rem;
    align-items: start;
    min-width: 0;
    padding: 1rem 1.25rem 2rem;
  }

  .prism-layout-navigator,
  .prism-layout-content {
    min-width: 0;
  }

  .prism-layout-header,
  .prism-layout-navigator,
  .prism-layout-footer {
    display: contents;
  }

  .prism-layout-content {
    min-height: 100%;
  }

  .prism-navigator {
    display: grid;
    align-content: start;
    gap: .85rem;
    min-width: 0;
    color: var(--prism-color-text);
  }

  .prism-navigator-sticky {
    position: sticky;
    top: 0;
  }

  .prism-navigator-header,
  .prism-navigator-footer {
    display: grid;
    gap: .25rem;
  }

  .prism-navigator-title {
    color: var(--prism-color-ink);
    font-size: .82rem;
    font-weight: 800;
  }

  .prism-navigator-description {
    color: var(--prism-color-text-muted);
    font-size: .76rem;
    line-height: 1.45;
  }

  .prism-navigator-body {
    min-width: 0;
  }

  .prism-footer {
    min-width: 0;
    border-top: 1px solid var(--prism-color-border);
    color: var(--prism-color-text-subtle);
  }

  .prism-footer-sticky {
    z-index: 40;
    background: color-mix(in srgb, var(--prism-color-surface-glass, #f7f4ff) 88%, transparent);
    backdrop-filter: blur(18px) saturate(1.2);
  }

  .prism-footer-inner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    min-width: 0;
    padding: 1.25rem;
  }

  .prism-footer-start,
  .prism-footer-end {
    display: flex;
    align-items: center;
    gap: .75rem;
    min-width: 0;
  }

  .prism-header {
    z-index: 40;
    width: 100%;
    border-bottom: 1px solid rgb(255 255 255 / 12%);
    background: color-mix(in srgb, var(--prism-color-surface-glass, #f7f4ff) 72%, transparent);
    box-shadow: 0 .45rem 1.6rem rgb(5 11 29 / 12%);
    backdrop-filter: blur(22px) saturate(1.25);
  }

  .prism-header-sticky {
    z-index: 40;
  }

  .prism-header-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    min-height: 3.7rem;
    padding: .7rem 1.2rem;
  }

  .prism-header-start,
  .prism-header-end {
    display: flex;
    align-items: center;
    gap: .85rem;
    min-width: 0;
  }

  .prism-header-start {
    flex: 1 1 auto;
  }

  .prism-header-end {
    flex: 0 1 auto;
    justify-content: flex-end;
  }

  @media (max-width: 48rem) {
    .prism-layout-body {
      grid-template-columns: minmax(0, 1fr);
    }

    .prism-navigator-sticky {
      position: static !important;
    }

    .prism-footer-inner {
      align-items: flex-start;
      flex-direction: column;
    }
  }

  .prism-background {
    --prism-background-base: #071427;
    --prism-background-accent: #3657d6;
    --prism-background-glow: #7ac7ff;
    --prism-background-overlay-opacity: .22;
    --prism-background-padding: 1.5rem;
    --prism-background-radius: 0;
    --prism-background-min-height: 18rem;
    --prism-background-height: auto;
    position: relative;
    display: block;
    min-height: var(--prism-background-min-height);
    height: var(--prism-background-height);
    overflow: hidden;
    border: 1px solid rgb(122 199 255 / 16%);
    border-radius: var(--prism-background-radius);
    background:
      radial-gradient(circle at 80% 12%, rgb(122 199 255 / 20%), transparent 12rem),
      radial-gradient(circle at 12% 0%, rgb(54 87 214 / 22%), transparent 10rem),
      linear-gradient(145deg, color-mix(in srgb, var(--prism-background-base) 88%, black), var(--prism-background-base) 72%);
    box-shadow: 0 1.1rem 2.4rem rgb(5 11 29 / 24%);
    isolation: isolate;
  }

  .prism-background-canvas,
  .prism-background-wash {
    position: absolute;
    inset: 0;
    pointer-events: none;
  }

  .prism-background-canvas {
    z-index: 0;
  }

  .prism-background-layer,
  .prism-background-fallback {
    position: absolute;
    inset: 0;
    display: block;
    width: 100%;
    height: 100%;
    border: 0;
  }

  .prism-background-layer {
    opacity: 1;
  }

  .prism-background-fallback,
  .prism-background-canvas[data-renderer="webgl"] .prism-background-fallback {
    display: none;
  }

  .prism-background-canvas[data-renderer="fallback"] .prism-background-layer {
    display: none;
  }

  .prism-background-canvas[data-renderer="fallback"] .prism-background-fallback {
    display: block;
  }

  .prism-background-wash {
    background:
      linear-gradient(180deg, rgb(3 7 18 / 16%), rgb(3 7 18 / 42%)),
      radial-gradient(circle at 18% 0%, color-mix(in srgb, var(--prism-background-glow) 20%, transparent), transparent 38%),
      radial-gradient(circle at 100% 100%, color-mix(in srgb, var(--prism-background-accent) 18%, transparent), transparent 32%);
    opacity: var(--prism-background-overlay-opacity);
    mix-blend-mode: screen;
  }

  .prism-background-content {
    position: relative;
    z-index: 1;
    display: grid;
    align-content: start;
    min-height: var(--prism-background-min-height);
    padding: var(--prism-background-padding);
    box-sizing: border-box;
  }

  .prism-background-midnight {
    border-color: rgb(122 199 255 / 18%);
  }

  .prism-background-aurora {
    border-color: rgb(109 94 247 / 20%);
  }

  .prism-background-tide {
    border-color: rgb(91 224 148 / 18%);
  }

  .prism-background-static .prism-background-layer,
  .prism-background-static .prism-background-wash {
    display: none;
  }

  .prism-label {
    --prism-label-size: var(--prism-font-size-lead);
    --prism-label-font: inherit;
    --prism-label-weight: 650;
    --prism-label-tracking: .01em;
    --prism-label-leading: 1.18;
    --prism-label-color: var(--prism-color-ink);
    --prism-label-stroke: #f4f0e6;
    display: inline-block;
    max-width: 100%;
    margin: 0;
    padding: 0;
    border: 0;
    color: var(--prism-label-color);
    font-family: var(--prism-label-font);
    font-size: var(--prism-label-size);
    font-weight: var(--prism-label-weight);
    letter-spacing: var(--prism-label-tracking);
    line-height: var(--prism-label-leading);
    vertical-align: baseline;
  }

  .prism-label-size-small {
    --prism-label-size: var(--prism-font-size-label);
    --prism-label-tracking: .06em;
    --prism-label-leading: 1.25;
  }

  .prism-label-size-medium {
    --prism-label-size: var(--prism-font-size-lead);
  }

  .prism-label-size-large {
    --prism-label-size: clamp(1.2rem, 2.2vw, 1.55rem);
    --prism-label-tracking: .01em;
  }

  .prism-label-size-display {
    --prism-label-size: clamp(1.45rem, 2.8vw, 2rem);
    --prism-label-tracking: -.01em;
    --prism-label-leading: 1.08;
  }

  .prism-label-font-sans {
    --prism-label-font: inherit;
  }

  .prism-label-font-serif {
    --prism-label-font: Georgia, 'Times New Roman', serif;
  }

  .prism-label-font-mono {
    --prism-label-font: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, 'Liberation Mono', monospace;
    --prism-label-tracking: -.03em;
  }

  .prism-label-weight-regular {
    --prism-label-weight: 500;
  }

  .prism-label-weight-medium {
    --prism-label-weight: 620;
  }

  .prism-label-weight-semibold {
    --prism-label-weight: 740;
  }

  .prism-label-weight-bold {
    --prism-label-weight: 830;
  }

  .prism-label-tone-ink {
    --prism-label-color: var(--prism-color-ink);
  }

  .prism-label-tone-muted {
    --prism-label-color: var(--prism-color-text-muted);
  }

  .prism-label-tone-accent {
    --prism-label-color: var(--prism-color-action);
  }

  .prism-label-tone-inverse {
    --prism-label-color: #f6f3ec;
  }

  .prism-label-always-visible {
    --prism-label-color: #0a1020;
    --prism-label-stroke: #f3eee4;
    position: relative;
    z-index: 2;
  }

  .prism-label-halo,
  .prism-label-face {
    display: block;
  }

  .prism-label-halo {
    position: absolute;
    inset: 0;
    color: transparent;
    -webkit-text-stroke: .09em var(--prism-label-stroke);
    pointer-events: none;
    user-select: none;
  }

  .prism-label-face {
    position: relative;
    color: var(--prism-label-color);
  }

  .prism-label-always-visible.prism-label-size-small .prism-label-halo {
    -webkit-text-stroke-width: .11em;
  }

  .prism-popup-layer {
    position: fixed;
    z-index: 200;
    inset: 0;
    display: grid;
    align-items: center;
    justify-items: center;
    padding: 1.25rem;
    isolation: isolate;
  }

  .prism-popup-placement-top {
    align-items: start;
    padding-top: min(8svh, 5rem);
  }

  .prism-popup-placement-bottom {
    align-items: end;
    padding-bottom: min(8svh, 5rem);
  }

  .prism-popup-backdrop {
    position: absolute;
    z-index: -1;
    inset: 0;
    display: block;
    background: rgb(8 13 29 / 62%);
    backdrop-filter: blur(12px);
    animation: prism-popup-backdrop-in .18s ease both;
  }

  .prism-popup-panel {
    display: grid;
    width: min(100%, 38rem);
    max-height: calc(100svh - 2.5rem);
    grid-template-rows: auto minmax(0, 1fr) auto;
    overflow: hidden;
    border: 1px solid color-mix(in srgb, var(--prism-color-focus) 25%, var(--prism-color-border-input));
    border-radius: calc(var(--prism-radius-surface) + .28rem);
    color: var(--prism-color-text);
    background:
      radial-gradient(circle at 92% -25%, var(--prism-color-focus-glow), transparent 18rem),
      color-mix(in srgb, var(--prism-color-surface) 97%, transparent);
    box-shadow: 0 2rem 5rem rgb(2 7 23 / 44%);
    outline: 0;
    animation: prism-popup-panel-in .24s cubic-bezier(.2, .8, .2, 1) both;
  }

  .prism-popup-small {
    width: min(100%, 26rem);
  }

  .prism-popup-large {
    width: min(100%, 56rem);
  }

  .prism-popup-full {
    width: 100%;
    height: calc(100svh - 2.5rem);
  }

  .prism-popup-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding: 1rem 1rem .9rem 1.15rem;
    border-bottom: 1px solid var(--prism-color-border-faint);
  }

  .prism-popup-heading {
    display: grid;
    min-width: 0;
    gap: .16rem;
  }

  .prism-popup-eyebrow {
    color: var(--prism-color-focus);
    font-size: .62rem;
    font-weight: 820;
    letter-spacing: .09em;
    text-transform: uppercase;
  }

  .prism-popup-title {
    overflow: hidden;
    color: var(--prism-color-ink);
    font-size: var(--prism-font-size-heading);
    font-weight: 820;
    letter-spacing: -.035em;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .prism-popup-description {
    color: var(--prism-color-text-subtle);
    font-size: var(--prism-font-size-small);
    line-height: 1.4;
  }

  .prism-popup-close {
    display: grid;
    width: 2.2rem;
    height: 2.2rem;
    flex: 0 0 2.2rem;
    place-items: center;
    padding: 0;
    border: 1px solid var(--prism-color-border-input);
    border-radius: .7rem;
    color: var(--prism-color-text-soft);
    background: var(--prism-color-surface);
    font: inherit;
    cursor: pointer;
    transition: border-color .18s ease, color .18s ease, background .18s ease, transform .18s ease;
  }

  .prism-popup-close:hover {
    border-color: var(--prism-color-focus);
    color: var(--prism-color-ink);
    background: var(--prism-color-lavender-surface);
    transform: translateY(-1px);
  }

  .prism-popup-close:focus-visible {
    outline: 2px solid var(--prism-color-focus);
    outline-offset: 2px;
  }

  .prism-popup-body {
    min-height: 0;
    padding: 1.15rem;
    overflow: auto;
    overscroll-behavior: contain;
  }

  .prism-popup-footer {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: .6rem;
    padding: .9rem 1.15rem;
    border-top: 1px solid var(--prism-color-border-faint);
    background: color-mix(in srgb, var(--prism-color-surface-tint) 50%, transparent);
  }

  @keyframes prism-popup-backdrop-in {
    from {
      opacity: 0;
    }
  }

  @keyframes prism-popup-panel-in {
    from {
      opacity: 0;
      transform: translateY(.7rem) scale(.98);
    }
  }

  .prism-popup-placement-top .prism-popup-panel {
    transform-origin: top center;
  }

  .prism-popup-placement-bottom .prism-popup-panel {
    transform-origin: bottom center;
  }

  @media (max-width: 38rem) {
    .prism-popup-layer {
      padding: .65rem;
    }

    .prism-popup-panel,
    .prism-popup-full {
      max-height: calc(100svh - 1.3rem);
    }

    .prism-popup-full {
      height: calc(100svh - 1.3rem);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .prism-popup-backdrop,
    .prism-popup-panel {
      animation: none;
    }
  }

  .prism-table {
    --prism-table-row-padding: .82rem;
    --prism-table-row-background: color-mix(in srgb, var(--prism-color-surface) 92%, transparent);
    position: relative;
    display: grid;
    min-width: 0;
    overflow: visible;
    border: 1px solid var(--prism-color-border-faint);
    border-radius: calc(var(--prism-radius-surface) + .18rem);
    color: var(--prism-color-text);
    background:
      radial-gradient(circle at 92% -35%, var(--prism-color-accent-glow), transparent 24rem),
      color-mix(in srgb, var(--prism-color-surface) 95%, transparent);
    box-shadow: var(--prism-shadow-card);
    isolation: isolate;
  }

  .prism-table-status {
    position: absolute;
    width: 1px;
    height: 1px;
    margin: -1px;
    padding: 0;
    overflow: hidden;
    clip: rect(0 0 0 0);
    white-space: nowrap;
    border: 0;
  }

  .prism-table-compact {
    --prism-table-row-padding: .54rem;
  }

  .prism-table-spacious {
    --prism-table-row-padding: 1.12rem;
  }

  .prism-table-toolbar,
  .prism-table-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding: .9rem 1rem;
  }

  .prism-table-toolbar {
    min-height: 4.5rem;
    border-bottom: 1px solid var(--prism-color-border-faint);
  }

  .prism-table-identity {
    display: grid;
    gap: .18rem;
    min-width: 0;
  }

  .prism-table-identity strong {
    overflow: hidden;
    color: var(--prism-color-ink);
    font-size: var(--prism-font-size-copy);
    font-weight: 820;
    letter-spacing: -.025em;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .prism-table-identity span {
    overflow: hidden;
    color: var(--prism-color-text-subtle);
    font-size: var(--prism-font-size-small);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .prism-table-toolbar-actions,
  .prism-table-pagination,
  .prism-table-pages,
  .prism-table-result-count {
    display: flex;
    align-items: center;
  }

  .prism-table-toolbar-actions {
    justify-content: flex-end;
    gap: .5rem;
    min-width: 0;
  }

  .prism-table-search {
    display: flex;
    width: min(19rem, 36vw);
    min-width: 10rem;
    align-items: center;
    gap: .52rem;
    padding: .58rem .72rem;
    border: 1px solid var(--prism-color-border-input);
    border-radius: .72rem;
    color: var(--prism-color-text-subtle);
    background: color-mix(in srgb, var(--prism-color-surface) 82%, transparent);
    box-shadow: inset 0 1px 0 var(--prism-color-white-soft);
    transition: border-color .18s ease, box-shadow .18s ease, background .18s ease;
  }

  .prism-table-search:focus-within {
    border-color: var(--prism-color-focus);
    background: var(--prism-color-surface);
    box-shadow: 0 0 0 .22rem var(--prism-color-focus-glow);
  }

  .prism-table-search input {
    width: 100%;
    min-width: 0;
    padding: 0;
    border: 0;
    outline: 0;
    color: var(--prism-color-ink);
    background: transparent;
    font: inherit;
    font-size: var(--prism-font-size-small);
  }

  .prism-table-search input::placeholder {
    color: var(--prism-color-text-subtle);
  }

  .prism-table-icon-button,
  .prism-table-page-arrow,
  .prism-table-page-number {
    display: inline-grid;
    width: 2.25rem;
    height: 2.25rem;
    flex: 0 0 2.25rem;
    place-items: center;
    padding: 0;
    border: 1px solid var(--prism-color-border-input);
    border-radius: .7rem;
    color: var(--prism-color-text-soft);
    background: color-mix(in srgb, var(--prism-color-surface) 88%, transparent);
    cursor: pointer;
    transition: border-color .18s ease, color .18s ease, background .18s ease, transform .18s ease, box-shadow .18s ease;
  }

  .prism-table-icon-button:hover,
  .prism-table-page-arrow:hover:not(:disabled),
  .prism-table-page-number:hover:not([data-active="true"]) {
    border-color: var(--prism-color-focus);
    color: var(--prism-color-ink);
    background: var(--prism-color-lavender-surface);
    transform: translateY(-1px);
  }

  .prism-table-icon-button:focus-visible,
  .prism-table-page-arrow:focus-visible,
  .prism-table-page-number:focus-visible,
  .prism-table-settings button:focus-visible,
  .prism-table-settings input:focus-visible,
  .prism-table-page-size select:focus-visible {
    outline: 2px solid var(--prism-color-focus);
    outline-offset: 2px;
  }

  .prism-table-settings-anchor {
    position: relative;
  }

  .prism-table-settings-scrim {
    position: fixed;
    z-index: 29;
    inset: 0;
    display: block;
    cursor: default;
  }

  .prism-table-settings {
    position: absolute;
    z-index: 30;
    top: calc(100% + .55rem);
    right: 0;
    display: grid;
    width: min(25rem, calc(100vw - 2rem));
    gap: .85rem;
    padding: .9rem;
    border: 1px solid var(--prism-color-border-input);
    border-radius: .9rem;
    color: var(--prism-color-text);
    background: color-mix(in srgb, var(--prism-color-surface) 97%, transparent);
    box-shadow: 0 1.25rem 3rem rgb(13 20 42 / 24%);
    backdrop-filter: blur(18px);
  }

  .prism-table-settings-heading,
  .prism-table-settings-column,
  .prism-table-settings-density {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: .75rem;
  }

  .prism-table-settings-heading > div {
    display: grid;
    gap: .12rem;
  }

  .prism-table-settings-heading strong {
    color: var(--prism-color-ink);
    font-size: var(--prism-font-size-body);
  }

  .prism-table-settings-heading span,
  .prism-table-settings-density > span {
    color: var(--prism-color-text-subtle);
    font-size: var(--prism-font-size-small);
  }

  .prism-table-settings-columns {
    display: grid;
    max-height: 17rem;
    overflow: auto;
    border: 1px solid var(--prism-color-border-faint);
    border-radius: .7rem;
  }

  .prism-table-settings-column {
    min-height: 2.55rem;
    padding: .38rem .48rem .38rem .68rem;
    border-bottom: 1px solid var(--prism-color-border-faint);
  }

  .prism-table-settings-column:last-child {
    border-bottom: 0;
  }

  .prism-table-settings-column label {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: .52rem;
    color: var(--prism-color-text-soft);
    font-size: var(--prism-font-size-small);
    font-weight: 680;
    cursor: pointer;
  }

  .prism-table-settings-column label span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .prism-table-settings-column-actions,
  .prism-table-settings-column-controls,
  .prism-table-settings-density div {
    display: inline-flex;
    align-items: center;
  }

  .prism-table-settings-column-controls {
    gap: .35rem;
  }

  .prism-table-settings-column-controls select {
    height: 1.8rem;
    padding: 0 1.25rem 0 .42rem;
    border: 1px solid var(--prism-color-border-input);
    border-radius: .45rem;
    color: var(--prism-color-text-soft);
    background: var(--prism-color-surface);
    font: inherit;
    font-size: .64rem;
    font-weight: 700;
    cursor: pointer;
  }

  .prism-table-settings-column-controls select:focus-visible {
    outline: 2px solid var(--prism-color-focus);
    outline-offset: 2px;
  }

  .prism-table-settings-column-actions {
    gap: .2rem;
  }

  .prism-table-settings-column-actions button {
    display: inline-grid;
    width: 1.75rem;
    height: 1.75rem;
    place-items: center;
    padding: 0;
    border: 0;
    border-radius: .45rem;
    color: var(--prism-color-text-subtle);
    background: transparent;
    cursor: pointer;
  }

  .prism-table-settings-column-actions button:hover:not(:disabled) {
    color: var(--prism-color-ink);
    background: var(--prism-color-lavender-surface);
  }

  .prism-table-settings button:disabled {
    opacity: .35;
    cursor: not-allowed;
  }

  .prism-table-settings-density div {
    overflow: hidden;
    border: 1px solid var(--prism-color-border-input);
    border-radius: .55rem;
  }

  .prism-table-settings-density button {
    padding: .4rem .52rem;
    border: 0;
    border-right: 1px solid var(--prism-color-border-input);
    color: var(--prism-color-text-subtle);
    background: transparent;
    font: inherit;
    font-size: .66rem;
    font-weight: 720;
    text-transform: capitalize;
    cursor: pointer;
  }

  .prism-table-settings-density button:last-child {
    border-right: 0;
  }

  .prism-table-settings-density button[data-active="true"] {
    color: var(--prism-color-ink);
    background: var(--prism-color-lavender-surface);
  }

  .prism-table-reset {
    padding: .55rem .7rem;
    border: 1px solid var(--prism-color-border-input);
    border-radius: .6rem;
    color: var(--prism-color-text-soft);
    background: transparent;
    font: inherit;
    font-size: var(--prism-font-size-small);
    font-weight: 700;
    cursor: pointer;
  }

  .prism-table-reset:hover {
    color: var(--prism-color-error);
    background: rgb(239 68 68 / 7%);
  }

  .prism-table-viewport {
    min-width: 0;
    max-height: 38rem;
    overflow: auto;
    overscroll-behavior: contain;
  }

  .prism-table table {
    width: 100%;
    min-width: max-content;
    border-spacing: 0;
    table-layout: fixed;
  }

  .prism-table-selection-column {
    width: 2.9rem;
  }

  .prism-table-head th {
    position: relative;
    z-index: 3;
    padding: .72rem var(--prism-table-row-padding);
    border-bottom: 1px solid var(--prism-color-border-input);
    color: var(--prism-color-text-subtle);
    background: color-mix(in srgb, var(--prism-color-surface-tint) 76%, var(--prism-color-surface));
    font-size: .68rem;
    font-weight: 800;
    letter-spacing: .075em;
    text-align: left;
    text-transform: uppercase;
    white-space: nowrap;
  }

  .prism-table-head-sticky th {
    position: sticky;
    top: 0;
  }

  .prism-table-header-cell[draggable="true"] {
    cursor: grab;
  }

  .prism-table-header-cell[data-dragging="true"] {
    opacity: .5;
    cursor: grabbing;
  }

  .prism-table-sort {
    display: flex;
    width: 100%;
    min-width: 0;
    align-items: center;
    justify-content: inherit;
    gap: .45rem;
    padding: 0;
    border: 0;
    color: inherit;
    background: transparent;
    font: inherit;
    letter-spacing: inherit;
    text-align: inherit;
    text-transform: inherit;
    cursor: pointer;
  }

  .prism-table-sort:disabled {
    cursor: default;
  }

  .prism-table-sort:focus-visible {
    outline: 2px solid var(--prism-color-focus);
    outline-offset: .35rem;
    border-radius: .2rem;
  }

  .prism-table-header-label {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .prism-table-sort-mark {
    position: relative;
    width: .48rem;
    height: .72rem;
    flex: 0 0 .48rem;
    opacity: .28;
  }

  .prism-table-sort-mark::before,
  .prism-table-sort-mark::after {
    position: absolute;
    left: .06rem;
    width: .3rem;
    height: .3rem;
    border-top: 1.5px solid currentColor;
    border-left: 1.5px solid currentColor;
    content: '';
  }

  .prism-table-sort-mark::before {
    top: .04rem;
    transform: rotate(45deg);
  }

  .prism-table-sort-mark::after {
    bottom: .04rem;
    transform: rotate(225deg);
  }

  .prism-table-sort-mark[data-direction="asc"],
  .prism-table-sort-mark[data-direction="desc"] {
    color: var(--prism-color-focus);
    opacity: 1;
  }

  .prism-table-sort-mark[data-direction="asc"]::after,
  .prism-table-sort-mark[data-direction="desc"]::before {
    opacity: .18;
  }

  .prism-table-resizer {
    position: absolute;
    z-index: 6;
    top: 22%;
    right: -.25rem;
    width: .5rem;
    height: 56%;
    border-radius: 999px;
    cursor: col-resize;
    touch-action: none;
  }

  .prism-table-resizer::after {
    position: absolute;
    top: 0;
    left: calc(50% - .5px);
    width: 1px;
    height: 100%;
    background: var(--prism-color-border-input);
    content: '';
    transition: width .15s ease, left .15s ease, background .15s ease;
  }

  .prism-table-resizer:hover::after,
  .prism-table-resizer:focus-visible::after {
    left: calc(50% - 1px);
    width: 2px;
    background: var(--prism-color-focus);
  }

  .prism-table-cell {
    box-sizing: border-box;
    min-width: 6rem;
  }

  .prism-table-body td {
    padding: var(--prism-table-row-padding);
    border-bottom: 1px solid var(--prism-color-border-faint);
    color: var(--prism-color-text-soft);
    background: var(--prism-table-row-background);
    font-size: var(--prism-font-size-small);
    vertical-align: middle;
    transition: color .16s ease, background .16s ease;
  }

  .prism-table-body tr:last-child td {
    border-bottom: 0;
  }

  .prism-table-virtual-spacer td {
    height: 0;
    padding: 0 !important;
    border: 0 !important;
    background: transparent !important;
  }

  .prism-table-striped .prism-table-body tr:nth-child(even) td {
    --prism-table-row-background: color-mix(in srgb, var(--prism-color-surface-tint) 38%, var(--prism-color-surface));
  }

  .prism-table-hoverable .prism-table-row:not(.prism-table-row-loading):hover td,
  .prism-table-row:focus-visible td {
    --prism-table-row-background: color-mix(in srgb, var(--prism-color-lavender-surface) 68%, var(--prism-color-surface));
    color: var(--prism-color-ink);
  }

  .prism-table-row-selected td {
    --prism-table-row-background: color-mix(in srgb, var(--prism-color-focus-glow) 48%, var(--prism-color-surface));
  }

  .prism-table-row-interactive {
    cursor: pointer;
  }

  .prism-table-row:focus-visible {
    outline: 2px solid var(--prism-color-focus);
    outline-offset: -2px;
  }

  .prism-table-cell-center,
  .prism-table-cell-center .prism-table-sort {
    justify-content: center;
    text-align: center;
  }

  .prism-table-cell-end,
  .prism-table-cell-end .prism-table-sort {
    justify-content: flex-end;
    text-align: right;
  }

  .prism-table-selection-cell {
    position: relative;
    z-index: 4;
    width: 2.9rem;
    min-width: 2.9rem;
    padding-right: .55rem !important;
    padding-left: .9rem !important;
    text-align: center;
  }

  .prism-table-selection-cell input,
  .prism-table-settings-column input {
    width: 1rem;
    height: 1rem;
    margin: 0;
    accent-color: var(--prism-color-focus);
    cursor: pointer;
  }

  .prism-table-cell-pinned {
    position: sticky !important;
    z-index: 4 !important;
    background: var(--prism-table-row-background) !important;
  }

  .prism-table-head .prism-table-cell-pinned {
    z-index: 7 !important;
    background: color-mix(in srgb, var(--prism-color-surface-tint) 76%, var(--prism-color-surface)) !important;
  }

  .prism-table-cell-pinned-left {
    box-shadow: 1px 0 var(--prism-color-border-faint);
  }

  .prism-table-cell-pinned-right {
    box-shadow: -1px 0 var(--prism-color-border-faint);
  }

  .prism-table-empty {
    height: 15rem;
    padding: 2rem !important;
    text-align: center;
  }

  .prism-table-empty > * {
    display: block;
    margin-inline: auto;
  }

  .prism-table-empty-mark {
    display: grid;
    width: 2.8rem;
    height: 2.8rem;
    place-items: center;
    margin-bottom: .7rem;
    border: 1px solid var(--prism-color-border-input);
    border-radius: .9rem;
    color: var(--prism-color-focus);
    background: var(--prism-color-lavender-surface);
    box-shadow: 0 .55rem 1.4rem var(--prism-color-focus-glow);
  }

  .prism-table-empty strong {
    margin-bottom: .3rem;
    color: var(--prism-color-ink);
    font-size: var(--prism-font-size-copy);
  }

  .prism-table-empty span:last-child {
    color: var(--prism-color-text-subtle);
  }

  .prism-table-error {
    gap: .45rem;
  }

  .prism-table-retry {
    margin-top: .3rem;
    padding: .45rem .7rem;
    border: 1px solid var(--prism-color-border-strong);
    border-radius: var(--prism-radius-control);
    color: var(--prism-color-ink);
    background: var(--prism-color-surface);
    font: inherit;
    font-size: var(--prism-font-size-small);
    font-weight: 750;
    cursor: pointer;
  }

  .prism-table-retry:hover {
    border-color: var(--prism-color-focus);
    color: var(--prism-color-focus);
  }

  .prism-table-skeleton {
    display: block;
    height: .72rem;
    border-radius: 999px;
    background: linear-gradient(90deg, var(--prism-color-border-faint), var(--prism-color-lavender-surface), var(--prism-color-border-faint));
    background-size: 220% 100%;
    animation: prism-table-shimmer 1.5s ease infinite;
  }

  .prism-table-skeleton-check {
    width: 1rem;
    height: 1rem;
    border-radius: .22rem;
  }

  @keyframes prism-table-shimmer {
    to {
      background-position: -220% 0;
    }
  }

  .prism-table-footer {
    min-height: 3.8rem;
    border-top: 1px solid var(--prism-color-border-faint);
  }

  .prism-table-result-count {
    gap: .28rem;
    color: var(--prism-color-text-subtle);
    font-size: var(--prism-font-size-small);
    white-space: nowrap;
  }

  .prism-table-result-count strong {
    color: var(--prism-color-ink);
  }

  .prism-table-filtered-count {
    margin-left: .35rem;
    padding: .18rem .4rem;
    border-radius: 999px;
    color: var(--prism-color-focus);
    background: var(--prism-color-focus-glow);
    font-size: .64rem;
    font-weight: 760;
  }

  .prism-table-pagination {
    gap: .8rem;
  }

  .prism-table-page-size {
    display: flex;
    align-items: center;
    gap: .42rem;
    color: var(--prism-color-text-subtle);
    font-size: var(--prism-font-size-small);
  }

  .prism-table-page-size select {
    height: 2.2rem;
    padding: 0 1.75rem 0 .65rem;
    border: 1px solid var(--prism-color-border-input);
    border-radius: .62rem;
    color: var(--prism-color-ink);
    background: var(--prism-color-surface);
    font: inherit;
    font-weight: 700;
    cursor: pointer;
  }

  .prism-table-pages {
    gap: .25rem;
  }

  .prism-table-page-number[data-active="true"] {
    border-color: transparent;
    color: var(--prism-color-white);
    background: var(--prism-color-focus);
    box-shadow: 0 .4rem .9rem var(--prism-color-focus-glow);
  }

  .prism-table-page-arrow:disabled {
    opacity: .32;
    cursor: not-allowed;
  }

  @media (max-width: 48rem) {
    .prism-table-toolbar,
    .prism-table-footer {
      align-items: stretch;
      flex-direction: column;
    }

    .prism-table-toolbar-actions,
    .prism-table-pagination {
      justify-content: space-between;
    }

    .prism-table-search {
      width: 100%;
    }

    .prism-table-result-count {
      justify-content: center;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .prism-table-skeleton {
      animation: none;
    }
  }

  .prism-badge {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 1.45rem;
    min-height: 1.35rem;
    padding: .18rem .46rem;
    border: 1px solid var(--prism-color-border-input);
    border-radius: 999px;
    color: var(--prism-color-text-soft);
    background: var(--prism-color-surface-raised);
    font-size: var(--prism-font-size-micro);
    font-weight: 760;
    letter-spacing: .04em;
    line-height: 1;
    white-space: nowrap;
  }

  .prism-badge-neutral {
    color: var(--prism-color-text-soft);
    background: var(--prism-color-surface-raised);
  }

  .prism-badge-success {
    border-color: var(--prism-color-success-glow);
    color: var(--prism-color-success);
    background: rgb(83 198 157 / 9%);
  }

  .prism-badge-info {
    border-color: rgb(14 165 233 / 24%);
    color: var(--prism-color-information);
    background: rgb(14 165 233 / 9%);
  }

  .prism-badge-warning {
    border-color: rgb(245 158 11 / 27%);
    color: var(--prism-color-warning);
    background: rgb(245 158 11 / 11%);
  }

  .prism-badge-error {
    border-color: rgb(239 68 68 / 25%);
    color: var(--prism-color-error);
    background: rgb(239 68 68 / 9%);
  }

  .prism-badge-small {
    min-height: 1.2rem;
    padding: .13rem .38rem;
    font-size: .62rem;
  }

  .prism-badge-large {
    min-height: 2rem;
    padding: .3rem .72rem;
    font-size: var(--prism-font-size-ui);
  }

  .prism-badge-pulse {
    animation: prism-badge-pulse .72s cubic-bezier(.2, .8, .3, 1);
  }

  @keyframes prism-badge-pulse {
    0% {
      box-shadow: 0 0 0 0 transparent;
      transform: scale(1);
    }

    38% {
      box-shadow: 0 0 0 .32rem var(--prism-color-focus-glow);
      transform: scale(1.12);
    }

    100% {
      box-shadow: 0 0 0 0 transparent;
      transform: scale(1);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .prism-badge-pulse {
      animation: none;
    }
  }

  .prism-pulse {
    display: inline-flex;
    align-items: center;
    gap: .52rem;
    min-height: 1.5rem;
    color: var(--prism-color-information);
    font-size: var(--prism-font-size-body);
    font-weight: 760;
    line-height: 1;
  }

  .prism-pulse-success {
    color: var(--prism-color-success);
  }

  .prism-pulse-success .prism-pulse-core {
    box-shadow: 0 0 0 .25rem var(--prism-color-success-glow);
  }

  .prism-pulse-info {
    color: var(--prism-color-information);
  }

  .prism-pulse-info .prism-pulse-core {
    box-shadow: 0 0 0 .25rem rgb(14 165 233 / 16%);
  }

  .prism-pulse-warning {
    color: var(--prism-color-warning);
  }

  .prism-pulse-warning .prism-pulse-core {
    box-shadow: 0 0 0 .25rem rgb(245 158 11 / 17%);
  }

  .prism-pulse-error {
    color: var(--prism-color-error);
  }

  .prism-pulse-error .prism-pulse-core {
    box-shadow: 0 0 0 .25rem rgb(239 68 68 / 16%);
  }

  .prism-pulse-off {
    color: var(--prism-color-text-subtle);
  }

  .prism-pulse-off .prism-pulse-core {
    box-shadow: 0 0 0 .25rem rgb(138 149 168 / 18%);
  }

  .prism-pulse-mark {
    position: relative;
    display: inline-grid;
    width: 1.8rem;
    height: 1.8rem;
    flex: 0 0 1.8rem;
    place-items: center;
  }

  .prism-pulse-mark::before,
  .prism-pulse-mark::after {
    position: absolute;
    inset: 0;
    border: 1px solid currentColor;
    border-radius: 50%;
    content: '';
    opacity: 0;
    transform: scale(.55);
    animation: prism-pulse-ring 2.6s cubic-bezier(.2, .7, .3, 1) infinite;
  }

  .prism-pulse-mark::after {
    animation-delay: 1.3s;
  }

  .prism-pulse-once .prism-pulse-mark::before {
    animation-iteration-count: 1;
  }

  .prism-pulse-once .prism-pulse-mark::after {
    display: none;
  }

  .prism-pulse-core {
    position: relative;
    z-index: 1;
    display: inline-grid;
    width: .45rem;
    height: .45rem;
    place-items: center;
    border-radius: 50%;
    background: currentColor;
  }

  .prism-pulse-small .prism-pulse-core {
    width: .38rem;
    height: .38rem;
  }

  .prism-pulse-large .prism-pulse-core {
    width: .58rem;
    height: .58rem;
  }

  .prism-pulse-off .prism-pulse-mark::before,
  .prism-pulse-off .prism-pulse-mark::after {
    display: none;
  }

  @keyframes prism-pulse-ring {
    0% {
      opacity: .5;
      transform: scale(.55);
    }

    70%,
    100% {
      opacity: 0;
      transform: scale(1.8);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .prism-pulse-mark::before,
    .prism-pulse-mark::after {
      opacity: .25;
      transform: scale(1);
      animation: none;
    }
  }

  .prism-tree-view {
    color: var(--prism-color-text);
  }

  .prism-tree-controls {
    display: grid;
    gap: .65rem;
    margin-bottom: .8rem;
    padding-bottom: .8rem;
    border-bottom: 1px solid var(--prism-color-border);
  }

  .prism-tree-filter {
    display: grid;
    gap: .4rem;
  }

  .prism-tree-filter-label {
    color: var(--prism-color-ink);
    font-size: .76rem;
    font-weight: 750;
  }

  .prism-tree-filter-input {
    width: 100%;
  }

  .prism-tree-actions {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: .65rem;
  }

  .prism-tree-count {
    min-width: 0;
    overflow: hidden;
    color: var(--prism-color-text-muted);
    font-size: .72rem;
    font-weight: 700;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .prism-tree-filter-empty {
    margin: 0 0 .8rem;
    padding: .65rem .7rem;
    border: 1px dashed var(--prism-color-border-strong);
    border-radius: var(--prism-radius-small);
    color: var(--prism-color-text-muted);
    font-size: .78rem;
  }

  .prism-tree-list {
    display: grid;
    gap: .35rem;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .prism-tree-list-nested {
    margin: .45rem 0 0 1rem;
    padding-left: .85rem;
    border-left: 1px solid var(--prism-color-border-faint);
  }

  .prism-tree-branch,
  .prism-tree-leaf {
    list-style: none;
  }

  .prism-tree-details {
    display: grid;
    gap: .35rem;
  }

  /* display:grid on details disables the UA hide-when-closed behavior */
  .prism-tree-details:not([open]) > :not(summary) {
    display: none;
  }

  .prism-tree-summary::-webkit-details-marker {
    display: none;
  }

  .prism-tree-summary::marker {
    content: '';
  }

  .prism-tree-summary,
  .prism-tree-link {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: .8rem;
    width: 100%;
    min-height: 2.4rem;
    padding: .58rem .68rem;
    border: 1px solid var(--prism-color-border);
    border-radius: .95rem;
    color: inherit;
    background: linear-gradient(180deg, var(--prism-color-white-strong), var(--prism-color-white-tint));
    box-shadow: 0 .22rem .58rem rgb(37 49 78 / 5%);
    text-align: left;
    text-decoration: none;
    cursor: pointer;
    transition: border-color .18s ease, box-shadow .18s ease, background .18s ease, transform .18s ease, color .18s ease;
  }

  .prism-tree-summary:hover,
  .prism-tree-link:hover {
    border-color: var(--prism-color-border-strong);
    box-shadow: 0 .38rem .92rem rgb(37 49 78 / 9%);
    transform: translateY(-1px);
  }

  .prism-tree-summary:focus-visible,
  .prism-tree-link:focus-visible {
    outline: 3px solid var(--prism-color-focus-glow);
    outline-offset: 3px;
  }

  .prism-tree-details[open] > .prism-tree-summary {
    border-color: var(--prism-color-lavender-border);
    background: linear-gradient(180deg, var(--prism-color-white), var(--prism-color-lavender-surface));
    box-shadow: 0 .45rem .95rem rgb(89 88 181 / 10%);
  }

  .prism-tree-entry-copy {
    display: inline-flex;
    align-items: center;
    gap: .62rem;
    min-width: 0;
  }

  .prism-tree-label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--prism-color-ink);
    font-size: var(--prism-font-size-body);
    font-weight: 700;
    letter-spacing: -.01em;
  }

  .prism-tree-toggle {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: .92rem;
    height: .92rem;
    flex: 0 0 .92rem;
    border: 1px solid var(--prism-color-border-input);
    border-radius: .32rem;
    background: linear-gradient(180deg, var(--prism-color-white), var(--prism-color-surface-tint));
    box-shadow: 0 .12rem .28rem rgb(37 49 78 / 7%);
    transition: border-color .18s ease, background .18s ease, box-shadow .18s ease, transform .18s ease;
  }

  .prism-tree-toggle-bar {
    position: absolute;
    top: 50%;
    left: 50%;
    border-radius: 999px;
    background: var(--prism-color-text-subtle);
    transform: translate(-50%, -50%);
    transition: background-color .18s ease, opacity .18s ease, transform .18s ease;
  }

  .prism-tree-toggle-bar-horizontal {
    width: .46rem;
    height: 1.5px;
  }

  .prism-tree-toggle-bar-vertical {
    width: 1.5px;
    height: .46rem;
  }

  .prism-tree-details[open] > .prism-tree-summary .prism-tree-toggle {
    border-color: var(--prism-color-lavender-border);
    background: linear-gradient(180deg, var(--prism-color-white), var(--prism-color-lavender-surface));
    box-shadow: 0 .18rem .4rem rgb(89 88 181 / 11%);
    transform: translateY(-1px);
  }

  .prism-tree-details[open] > .prism-tree-summary .prism-tree-toggle-bar {
    background: var(--prism-color-action);
  }

  .prism-tree-details[open] > .prism-tree-summary .prism-tree-toggle-bar-vertical {
    opacity: 0;
    transform: scaleY(.45);
  }

  .prism-tree-marker,
  .prism-tree-dot {
    width: .5rem;
    height: .5rem;
    flex: 0 0 .5rem;
    border-radius: 999px;
    background: linear-gradient(135deg, var(--prism-color-action), var(--prism-color-accent-bright));
    box-shadow: 0 0 0 .22rem var(--prism-color-focus-glow);
  }

  .prism-tree-dot {
    width: .42rem;
    height: .42rem;
    flex-basis: .42rem;
    box-shadow: 0 0 0 .16rem rgb(78 115 234 / 12%);
  }

  .prism-tree-summary-active,
  .prism-tree-details[open] > .prism-tree-summary-active,
  .prism-tree-link-active {
    border-color: var(--prism-color-lavender-border);
    background: linear-gradient(180deg, var(--prism-color-white), var(--prism-color-lavender-surface));
    box-shadow: 0 .52rem 1rem rgb(89 88 181 / 11%);
  }

  .prism-tree-summary-active:hover,
  .prism-tree-details[open] > .prism-tree-summary-active:hover,
  .prism-tree-link-active:hover {
    border-color: var(--prism-color-focus);
    background: linear-gradient(180deg, var(--prism-color-white), var(--prism-color-accent-soft));
    box-shadow: 0 .72rem 1.35rem rgb(89 88 181 / 18%);
    transform: translateY(-1px);
  }

  .prism-tree-summary-active .prism-tree-label,
  .prism-tree-link-active .prism-tree-label {
    color: var(--prism-color-ink);
  }

  .prism-tree-model-aurora {
    padding: .72rem;
    border: 1px solid rgb(137 119 246 / 18%);
    border-radius: 1.45rem;
    background:
      radial-gradient(circle at 8% 0%, rgb(104 211 255 / 17%), transparent 9rem),
      linear-gradient(135deg, rgb(255 255 255 / 82%), rgb(245 240 255 / 94%));
    box-shadow: 0 1.2rem 2.1rem rgb(109 94 247 / 10%);
  }

  .prism-tree-model-aurora .prism-tree-list-nested {
    border-left-color: rgb(137 119 246 / 25%);
  }

  .prism-tree-model-aurora .prism-tree-summary,
  .prism-tree-model-aurora .prism-tree-link {
    border-color: rgb(137 119 246 / 18%);
    border-radius: 1.15rem;
    background: linear-gradient(135deg, rgb(255 255 255 / 88%), rgb(247 243 255 / 82%));
    box-shadow: 0 .4rem 1rem rgb(109 94 247 / 8%);
  }

  .prism-tree-model-aurora .prism-tree-summary:hover,
  .prism-tree-model-aurora .prism-tree-link:hover {
    border-color: rgb(109 94 247 / 42%);
    box-shadow: 0 .55rem 1.2rem rgb(109 94 247 / 14%);
  }

  .prism-tree-model-aurora .prism-tree-details[open] > .prism-tree-summary {
    border-color: rgb(109 94 247 / 38%);
    background: linear-gradient(135deg, rgb(255 255 255 / 96%), rgb(237 233 255 / 95%));
    box-shadow: 0 .6rem 1.2rem rgb(109 94 247 / 14%);
  }

  .prism-tree-model-aurora .prism-tree-toggle {
    border-color: rgb(137 119 246 / 28%);
    background: rgb(255 255 255 / 72%);
  }

  .prism-tree-model-aurora .prism-tree-marker,
  .prism-tree-model-aurora .prism-tree-dot {
    background: linear-gradient(135deg, #836ef5, #58c9c2);
  }

  .prism-tree-model-aurora .prism-tree-summary-active,
  .prism-tree-model-aurora .prism-tree-details[open] > .prism-tree-summary-active,
  .prism-tree-model-aurora .prism-tree-link-active {
    border-color: rgb(109 94 247 / 42%);
    background: linear-gradient(135deg, rgb(255 255 255 / 96%), rgb(232 228 255 / 96%));
    box-shadow: 0 .58rem 1.15rem rgb(109 94 247 / 16%);
  }

  .prism-tree-model-aurora .prism-tree-summary-active:hover,
  .prism-tree-model-aurora .prism-tree-details[open] > .prism-tree-summary-active:hover,
  .prism-tree-model-aurora .prism-tree-link-active:hover {
    border-color: rgb(109 94 247 / 58%);
    background: linear-gradient(135deg, rgb(255 255 255 / 99%), rgb(223 217 255 / 98%));
    box-shadow: 0 .8rem 1.45rem rgb(109 94 247 / 24%);
  }

  .prism-tree-model-nocturne {
    padding: .9rem;
    border: 1px solid rgb(139 169 255 / 25%);
    border-radius: 1.45rem;
    color: #dce8ff;
    background:
      radial-gradient(circle at 88% 4%, rgb(111 139 255 / 19%), transparent 11rem),
      linear-gradient(145deg, #172344, #0b1126 72%);
    box-shadow: 0 1.25rem 2.2rem rgb(9 16 43 / 28%);
  }

  .prism-tree-model-nocturne .prism-tree-list-nested {
    border-left-color: rgb(139 169 255 / 28%);
  }

  .prism-tree-model-nocturne .prism-tree-summary,
  .prism-tree-model-nocturne .prism-tree-link {
    border-color: rgb(139 169 255 / 24%);
    color: #dce8ff;
    background: linear-gradient(180deg, rgb(38 55 96 / 88%), rgb(23 35 69 / 92%));
    box-shadow: 0 .28rem .75rem rgb(3 8 25 / 20%);
  }

  .prism-tree-model-nocturne .prism-tree-summary:hover,
  .prism-tree-model-nocturne .prism-tree-link:hover {
    border-color: rgb(154 186 255 / 58%);
    background: linear-gradient(180deg, rgb(48 68 116 / 92%), rgb(27 42 80 / 94%));
    box-shadow: 0 .48rem 1rem rgb(3 8 25 / 32%);
  }

  .prism-tree-model-nocturne .prism-tree-details[open] > .prism-tree-summary {
    border-color: rgb(131 213 255 / 56%);
    background: linear-gradient(180deg, rgb(44 68 116 / 96%), rgb(25 48 83 / 98%));
    box-shadow: 0 .5rem 1.2rem rgb(3 8 25 / 32%);
  }

  .prism-tree-model-nocturne .prism-tree-label {
    color: #f1f6ff;
  }

  .prism-tree-model-nocturne .prism-tree-toggle {
    border-color: rgb(139 169 255 / 44%);
    background: rgb(13 23 51 / 72%);
  }

  .prism-tree-model-nocturne .prism-tree-details[open] > .prism-tree-summary .prism-tree-toggle {
    border-color: rgb(131 213 255 / 56%);
    background: rgb(13 23 51 / 82%);
    box-shadow: 0 .18rem .4rem rgb(3 8 25 / 28%);
  }

  .prism-tree-model-nocturne .prism-tree-toggle-bar {
    background: #a5dfff;
  }

  .prism-tree-model-nocturne .prism-tree-details[open] > .prism-tree-summary .prism-tree-toggle-bar {
    background: #a5dfff;
  }

  .prism-tree-model-nocturne .prism-tree-marker,
  .prism-tree-model-nocturne .prism-tree-dot {
    background: linear-gradient(135deg, #80d9ff, #b09cff);
    box-shadow: 0 0 0 .22rem rgb(128 217 255 / 14%);
  }

  .prism-tree-model-nocturne .prism-tree-meta {
    border-color: rgb(128 217 255 / 30%);
    color: #bceaff;
    background: rgb(128 217 255 / 11%);
  }

  .prism-tree-model-nocturne .prism-tree-summary-active,
  .prism-tree-model-nocturne .prism-tree-details[open] > .prism-tree-summary-active,
  .prism-tree-model-nocturne .prism-tree-link-active {
    border-color: rgb(177 155 255 / 62%);
    background: linear-gradient(135deg, rgb(75 67 143 / 80%), rgb(40 63 107 / 92%));
    box-shadow: 0 .5rem 1.15rem rgb(93 78 203 / 25%);
  }

  .prism-tree-model-nocturne .prism-tree-summary-active:hover,
  .prism-tree-model-nocturne .prism-tree-details[open] > .prism-tree-summary-active:hover,
  .prism-tree-model-nocturne .prism-tree-link-active:hover {
    border-color: rgb(193 228 255 / 72%);
    background: linear-gradient(135deg, rgb(91 81 166 / 86%), rgb(47 76 126 / 96%));
    box-shadow: 0 .72rem 1.4rem rgb(93 78 203 / 34%);
  }

  .prism-tree-model-editorial {
    padding: .35rem .7rem .35rem .85rem;
    border-left: 3px solid var(--prism-color-peach-border);
    background: linear-gradient(90deg, rgb(255 248 244 / 74%), transparent 82%);
  }

  .prism-tree-model-editorial .prism-tree-list-nested {
    margin-left: .45rem;
    padding-left: 1.15rem;
    border-left: 1px solid var(--prism-color-peach-border);
  }

  .prism-tree-model-editorial .prism-tree-summary,
  .prism-tree-model-editorial .prism-tree-link {
    min-height: 2.65rem;
    padding: .5rem .25rem;
    border: 0;
    border-bottom: 1px solid var(--prism-color-border);
    border-radius: 0;
    background: transparent;
    box-shadow: none;
  }

  .prism-tree-model-editorial .prism-tree-summary:hover,
  .prism-tree-model-editorial .prism-tree-link:hover {
    border-color: var(--prism-color-peach-border);
    background: linear-gradient(90deg, rgb(255 240 234 / 70%), transparent);
    box-shadow: none;
    transform: none;
  }

  .prism-tree-model-editorial .prism-tree-details[open] > .prism-tree-summary {
    border-color: var(--prism-color-accent);
    background: linear-gradient(90deg, rgb(255 240 234 / 78%), transparent);
    box-shadow: none;
  }

  .prism-tree-model-editorial .prism-tree-toggle {
    border: 0;
    background: transparent;
    box-shadow: none;
  }

  .prism-tree-model-editorial .prism-tree-details[open] > .prism-tree-summary .prism-tree-toggle {
    border: 0;
    background: transparent;
    box-shadow: none;
    transform: none;
  }

  .prism-tree-model-editorial .prism-tree-details[open] > .prism-tree-summary .prism-tree-toggle-bar {
    background: var(--prism-color-accent);
  }

  .prism-tree-model-editorial .prism-tree-marker,
  .prism-tree-model-editorial .prism-tree-dot {
    width: .38rem;
    height: .38rem;
    flex-basis: .38rem;
    background: var(--prism-color-accent-bright);
    box-shadow: 0 0 0 .17rem var(--prism-color-accent-glow);
  }

  .prism-tree-model-editorial .prism-tree-summary-active,
  .prism-tree-model-editorial .prism-tree-details[open] > .prism-tree-summary-active,
  .prism-tree-model-editorial .prism-tree-link-active {
    border-bottom-color: var(--prism-color-accent);
    background: linear-gradient(90deg, rgb(255 240 234 / 85%), transparent);
    box-shadow: none;
  }

  .prism-tree-model-editorial .prism-tree-summary-active:hover,
  .prism-tree-model-editorial .prism-tree-details[open] > .prism-tree-summary-active:hover,
  .prism-tree-model-editorial .prism-tree-link-active:hover {
    border-bottom-color: var(--prism-color-accent-hover);
    background: linear-gradient(90deg, rgb(255 233 225 / 94%), transparent 86%);
    box-shadow: inset 0 -1px 0 rgb(231 111 81 / 18%);
    transform: none;
  }

  .prism-tree-model-terminal {
    padding: .8rem;
    border: 1px solid rgb(91 224 148 / 23%);
    border-radius: 1.15rem;
    color: #baf5cb;
    background:
      radial-gradient(circle at 100% 0%, rgb(49 177 113 / 12%), transparent 10rem),
      linear-gradient(145deg, #101c1b, #091112 74%);
    box-shadow: 0 1.1rem 2rem rgb(4 14 13 / 25%);
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
  }

  .prism-tree-model-terminal .prism-tree-list-nested {
    border-left-color: rgb(91 224 148 / 30%);
  }

  .prism-tree-model-terminal .prism-tree-summary,
  .prism-tree-model-terminal .prism-tree-link {
    min-height: 2.3rem;
    padding: .48rem .62rem;
    border-color: transparent;
    border-radius: .58rem;
    color: #baf5cb;
    background: transparent;
    box-shadow: none;
  }

  .prism-tree-model-terminal .prism-tree-summary:hover,
  .prism-tree-model-terminal .prism-tree-link:hover {
    border-color: rgb(91 224 148 / 28%);
    background: rgb(91 224 148 / 9%);
    box-shadow: none;
    transform: translateX(.12rem);
  }

  .prism-tree-model-terminal .prism-tree-details[open] > .prism-tree-summary {
    border-color: rgb(91 224 148 / 34%);
    background: rgb(91 224 148 / 13%);
    box-shadow: inset 3px 0 #5be094;
  }

  .prism-tree-model-terminal .prism-tree-details[open] > .prism-tree-summary .prism-tree-toggle {
    border-color: rgb(91 224 148 / 38%);
    background: rgb(91 224 148 / 7%);
    box-shadow: none;
    transform: none;
  }

  .prism-tree-model-terminal .prism-tree-details[open] > .prism-tree-summary .prism-tree-toggle-bar {
    background: #5be094;
  }

  .prism-tree-model-terminal .prism-tree-label {
    color: #d8ffe4;
    font-weight: 650;
    letter-spacing: .01em;
  }

  .prism-tree-model-terminal .prism-tree-toggle {
    border-color: rgb(91 224 148 / 38%);
    border-radius: .28rem;
    background: rgb(91 224 148 / 7%);
    box-shadow: none;
  }

  .prism-tree-model-terminal .prism-tree-toggle-bar {
    background: #5be094;
  }

  .prism-tree-model-terminal .prism-tree-marker,
  .prism-tree-model-terminal .prism-tree-dot {
    width: .38rem;
    height: .38rem;
    flex-basis: .38rem;
    background: #5be094;
    box-shadow: 0 0 0 .16rem rgb(91 224 148 / 12%);
  }

  .prism-tree-model-terminal .prism-tree-meta {
    border-color: rgb(91 224 148 / 28%);
    color: #8deeb0;
    background: rgb(91 224 148 / 9%);
    font-family: inherit;
  }

  .prism-tree-model-terminal .prism-tree-summary-active,
  .prism-tree-model-terminal .prism-tree-details[open] > .prism-tree-summary-active,
  .prism-tree-model-terminal .prism-tree-link-active {
    border-color: rgb(91 224 148 / 38%);
    background: rgb(91 224 148 / 14%);
    box-shadow: inset 3px 0 #5be094;
  }

  .prism-tree-model-terminal .prism-tree-summary-active:hover,
  .prism-tree-model-terminal .prism-tree-details[open] > .prism-tree-summary-active:hover,
  .prism-tree-model-terminal .prism-tree-link-active:hover {
    border-color: rgb(120 239 172 / 54%);
    background: rgb(91 224 148 / 18%);
    box-shadow: inset 3px 0 #5be094, 0 .24rem .62rem rgb(4 14 13 / 22%);
    transform: translateX(.12rem);
  }

  .prism-tree-view.prism-tree-items-minimal {
    padding: 0;
    border: 0;
    border-radius: 0;
    background: transparent;
    box-shadow: none;
  }

  .prism-tree-items-minimal .prism-tree-list,
  .prism-tree-items-minimal .prism-tree-details {
    gap: .08rem;
  }

  .prism-tree-items-minimal .prism-tree-list-nested {
    margin-top: .2rem;
  }

  .prism-tree-items-minimal .prism-tree-summary,
  .prism-tree-items-minimal .prism-tree-link,
  .prism-tree-items-minimal .prism-tree-summary:not(.prism-tree-summary-active):hover,
  .prism-tree-items-minimal .prism-tree-link:not(.prism-tree-link-active):hover,
  .prism-tree-items-minimal .prism-tree-details[open] > .prism-tree-summary:not(.prism-tree-summary-active) {
    min-height: 2.2rem;
    padding: .46rem .55rem;
    border: 0;
    border-radius: 0;
    background: transparent;
    box-shadow: none;
    transform: none;
  }

  .prism-tree-items-minimal .prism-tree-summary,
  .prism-tree-items-minimal .prism-tree-link {
    box-sizing: border-box;
  }

  .prism-tree-items-minimal .prism-tree-summary-active,
  .prism-tree-items-minimal .prism-tree-details[open] > .prism-tree-summary-active,
  .prism-tree-items-minimal .prism-tree-link-active {
    border: 1px solid var(--prism-color-lavender-border);
    border-radius: .68rem;
    background: var(--prism-color-lavender-surface);
    box-shadow: 0 .28rem .72rem rgb(89 88 181 / 9%);
  }

  .prism-tree-items-minimal .prism-tree-summary-active:hover,
  .prism-tree-items-minimal .prism-tree-details[open] > .prism-tree-summary-active:hover,
  .prism-tree-items-minimal .prism-tree-link-active:hover {
    border-color: var(--prism-color-focus);
    background: linear-gradient(180deg, rgb(255 255 255 / 96%), var(--prism-color-lavender-surface));
    box-shadow: 0 .42rem .95rem rgb(89 88 181 / 15%);
    transform: translateY(-1px);
  }

  .prism-tree-items-minimal .prism-tree-summary:hover .prism-tree-label,
  .prism-tree-items-minimal .prism-tree-link:hover .prism-tree-label,
  .prism-tree-items-minimal .prism-tree-details[open] > .prism-tree-summary .prism-tree-label,
  .prism-tree-items-minimal .prism-tree-summary-active .prism-tree-label,
  .prism-tree-items-minimal .prism-tree-link-active .prism-tree-label {
    color: var(--prism-color-action);
  }

  .prism-tree-model-nocturne.prism-tree-items-minimal .prism-tree-summary:hover .prism-tree-label,
  .prism-tree-model-nocturne.prism-tree-items-minimal .prism-tree-link:hover .prism-tree-label,
  .prism-tree-model-nocturne.prism-tree-items-minimal .prism-tree-details[open] > .prism-tree-summary .prism-tree-label,
  .prism-tree-model-nocturne.prism-tree-items-minimal .prism-tree-summary-active .prism-tree-label,
  .prism-tree-model-nocturne.prism-tree-items-minimal .prism-tree-link-active .prism-tree-label {
    color: #80d9ff;
  }

  .prism-tree-model-nocturne.prism-tree-items-minimal .prism-tree-summary-active,
  .prism-tree-model-nocturne.prism-tree-items-minimal .prism-tree-details[open] > .prism-tree-summary-active,
  .prism-tree-model-nocturne.prism-tree-items-minimal .prism-tree-link-active {
    border-color: rgb(128 217 255 / 48%);
    background: rgb(42 61 106 / 78%);
    box-shadow: 0 .3rem .8rem rgb(3 8 25 / 24%);
  }

  .prism-tree-model-nocturne.prism-tree-items-minimal .prism-tree-summary-active:hover,
  .prism-tree-model-nocturne.prism-tree-items-minimal .prism-tree-details[open] > .prism-tree-summary-active:hover,
  .prism-tree-model-nocturne.prism-tree-items-minimal .prism-tree-link-active:hover {
    border-color: rgb(165 231 255 / 64%);
    background: rgb(52 74 126 / 86%);
    box-shadow: 0 .42rem .98rem rgb(3 8 25 / 30%);
  }

  .prism-tree-model-aurora.prism-tree-items-minimal .prism-tree-summary-active,
  .prism-tree-model-aurora.prism-tree-items-minimal .prism-tree-details[open] > .prism-tree-summary-active,
  .prism-tree-model-aurora.prism-tree-items-minimal .prism-tree-link-active {
    border-color: rgb(109 94 247 / 38%);
    background: rgb(237 233 255 / 88%);
    box-shadow: 0 .3rem .8rem rgb(109 94 247 / 10%);
  }

  .prism-tree-model-aurora.prism-tree-items-minimal .prism-tree-summary-active:hover,
  .prism-tree-model-aurora.prism-tree-items-minimal .prism-tree-details[open] > .prism-tree-summary-active:hover,
  .prism-tree-model-aurora.prism-tree-items-minimal .prism-tree-link-active:hover {
    border-color: rgb(109 94 247 / 52%);
    background: rgb(229 223 255 / 94%);
    box-shadow: 0 .45rem 1rem rgb(109 94 247 / 16%);
  }

  .prism-tree-model-editorial.prism-tree-items-minimal .prism-tree-summary-active,
  .prism-tree-model-editorial.prism-tree-items-minimal .prism-tree-details[open] > .prism-tree-summary-active,
  .prism-tree-model-editorial.prism-tree-items-minimal .prism-tree-link-active {
    border-color: var(--prism-color-peach-border);
    background: rgb(255 240 234 / 72%);
    box-shadow: 0 .28rem .72rem rgb(180 91 62 / 8%);
  }

  .prism-tree-model-editorial.prism-tree-items-minimal .prism-tree-summary-active:hover,
  .prism-tree-model-editorial.prism-tree-items-minimal .prism-tree-details[open] > .prism-tree-summary-active:hover,
  .prism-tree-model-editorial.prism-tree-items-minimal .prism-tree-link-active:hover {
    border-color: var(--prism-color-accent);
    background: rgb(255 234 226 / 86%);
    box-shadow: 0 .34rem .82rem rgb(180 91 62 / 12%);
    transform: none;
  }

  .prism-tree-model-terminal.prism-tree-items-minimal .prism-tree-summary:hover .prism-tree-label,
  .prism-tree-model-terminal.prism-tree-items-minimal .prism-tree-link:hover .prism-tree-label,
  .prism-tree-model-terminal.prism-tree-items-minimal .prism-tree-details[open] > .prism-tree-summary .prism-tree-label,
  .prism-tree-model-terminal.prism-tree-items-minimal .prism-tree-summary-active .prism-tree-label,
  .prism-tree-model-terminal.prism-tree-items-minimal .prism-tree-link-active .prism-tree-label {
    color: #5be094;
  }

  .prism-tree-model-terminal.prism-tree-items-minimal .prism-tree-summary-active,
  .prism-tree-model-terminal.prism-tree-items-minimal .prism-tree-details[open] > .prism-tree-summary-active,
  .prism-tree-model-terminal.prism-tree-items-minimal .prism-tree-link-active {
    border-color: rgb(91 224 148 / 38%);
    background: rgb(91 224 148 / 11%);
    box-shadow: 0 .28rem .72rem rgb(4 14 13 / 20%);
  }

  .prism-tree-model-terminal.prism-tree-items-minimal .prism-tree-summary-active:hover,
  .prism-tree-model-terminal.prism-tree-items-minimal .prism-tree-details[open] > .prism-tree-summary-active:hover,
  .prism-tree-model-terminal.prism-tree-items-minimal .prism-tree-link-active:hover {
    border-color: rgb(91 224 148 / 56%);
    background: rgb(91 224 148 / 16%);
    box-shadow: 0 .4rem .9rem rgb(4 14 13 / 24%);
    transform: translateX(.12rem);
  }

  :where(
    .prism-form-field,
    .prism-auto-complete,
    .prism-color-picker,
    .prism-file-picker,
    .prism-date-picker,
    .prism-date-time-picker,
    .prism-alert,
    .prism-toast-region,
    .prism-dropdown,
    .prism-menu,
    .prism-popover-anchor,
    .prism-tooltip-anchor,
    .prism-tabs,
    .prism-progress,
    .prism-spinner,
    .prism-skeleton,
    .prism-empty-state,
    .prism-pagination,
    .prism-avatar,
    .prism-tag,
    .prism-separator,
    .prism-stack,
    .prism-grid
  ) {
    --prism-color-text-strong: var(--prism-color-ink);
    --prism-color-text-inverse: var(--prism-color-white);
    --prism-color-surface-raised: var(--prism-color-surface-card);
    --prism-color-surface-hover: var(--prism-color-surface-tint);
    --prism-color-danger: var(--prism-color-error);
    --prism-radius-small: 0.45rem;
    --prism-radius-medium: var(--prism-radius-control);
    --prism-shadow-floating: var(--prism-shadow-card);
  }

  .prism-form-field {
    display: grid;
    gap: 0.45rem;
  }

  .prism-form-field-label {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    color: var(--prism-color-text-strong);
    font-size: 0.85rem;
    font-weight: 700;
  }

  .prism-form-field-required,
  .prism-form-field-error {
    color: var(--prism-color-danger);
  }

  .prism-form-field-hint,
  .prism-form-field-error {
    font-size: 0.78rem;
    line-height: 1.4;
  }

  .prism-form-field-hint {
    color: var(--prism-color-text-muted);
  }

  .prism-form-field-error {
    font-weight: 600;
  }

  .prism-color-picker {
    display: grid;
    gap: 0.45rem;
    width: max-content;
    max-width: 100%;
  }

  .prism-color-picker-label {
    color: var(--prism-color-text-strong);
    font-size: 0.85rem;
    font-weight: 700;
  }

  .prism-color-picker-control {
    display: inline-flex;
    align-items: center;
    gap: 0.65rem;
  }

  .prism-color-picker-input {
    display: block;
    width: 3.15rem;
    height: 2.35rem;
    padding: 0.2rem;
    border: 1px solid var(--prism-color-border-strong);
    border-radius: var(--prism-radius-medium);
    background: var(--prism-color-surface-raised);
    cursor: pointer;
  }

  .prism-color-picker-input:hover {
    border-color: var(--prism-color-accent);
  }

  .prism-color-picker-input:focus-visible {
    outline: 2px solid var(--prism-color-focus);
    outline-offset: 2px;
  }

  .prism-color-picker-input:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .prism-color-picker-small .prism-color-picker-input {
    width: 2.65rem;
    height: 1.95rem;
  }

  .prism-color-picker-large .prism-color-picker-input {
    width: 3.65rem;
    height: 2.75rem;
  }

  .prism-color-picker-value {
    color: var(--prism-color-text-muted);
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 0.78rem;
    letter-spacing: 0.03em;
  }

  .prism-file-picker {
    display: grid;
    gap: 0.45rem;
    width: min(100%, 28rem);
  }

  .prism-file-picker-label {
    color: var(--prism-color-text-strong);
    font-size: 0.85rem;
    font-weight: 700;
  }

  .prism-file-picker-control {
    position: relative;
  }

  .prism-file-picker-input {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
    white-space: nowrap;
    border: 0;
  }

  .prism-file-picker-trigger {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    align-items: center;
    gap: 0.7rem;
    min-height: 2.8rem;
    padding: 0.45rem 0.55rem;
    border: 1px solid var(--prism-color-border-strong);
    border-radius: var(--prism-radius-medium);
    background: var(--prism-color-surface-raised);
    cursor: pointer;
    transition: border-color 0.18s ease, background 0.18s ease, box-shadow 0.18s ease;
  }

  .prism-file-picker-trigger:hover {
    border-color: var(--prism-color-accent);
    background: var(--prism-color-surface-hover);
  }

  .prism-file-picker-input:focus-visible + .prism-file-picker-trigger {
    border-color: var(--prism-color-focus);
    outline: 2px solid var(--prism-color-focus);
    outline-offset: 2px;
    box-shadow: 0 0 0 0.25rem var(--prism-color-focus-glow);
  }

  .prism-file-picker-action {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    min-height: 1.95rem;
    padding: 0.35rem 0.65rem;
    border-radius: var(--prism-radius-small);
    color: var(--prism-color-white);
    background: linear-gradient(135deg, var(--prism-color-action), var(--prism-color-action-active));
    font-size: 0.78rem;
    font-weight: 750;
    white-space: nowrap;
  }

  .prism-file-picker-action .prism-icon {
    flex: 0 0 auto;
  }

  .prism-file-picker-summary {
    min-width: 0;
    overflow: hidden;
    color: var(--prism-color-text);
    font-size: 0.82rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .prism-file-picker-summary-empty {
    color: var(--prism-color-text-muted);
  }

  .prism-file-picker:has(.prism-file-picker-input:disabled) .prism-file-picker-trigger {
    opacity: var(--prism-button-disabled-opacity);
    cursor: not-allowed;
  }

  .prism-file-picker-invalid .prism-file-picker-trigger {
    border-color: var(--prism-color-error);
  }

  .prism-file-picker-message {
    display: block;
    margin-top: 0.35rem;
    color: var(--prism-color-text-subtle);
    font-size: var(--prism-font-size-small);
  }

  .prism-file-picker-message-error {
    color: var(--prism-color-error);
  }

  .prism-file-picker-small .prism-file-picker-trigger {
    min-height: 2.25rem;
    padding: 0.35rem 0.45rem;
  }

  .prism-file-picker-small .prism-file-picker-action {
    min-height: 1.65rem;
    padding: 0.28rem 0.5rem;
    font-size: var(--prism-font-size-small);
  }

  .prism-file-picker-large .prism-file-picker-trigger {
    min-height: 3.15rem;
    padding: 0.55rem 0.65rem;
  }

  .prism-file-picker-large .prism-file-picker-action {
    min-height: 2.25rem;
    padding: 0.45rem 0.75rem;
    font-size: var(--prism-font-size-body);
  }

  .prism-date-picker,
  .prism-date-time-picker {
    position: relative;
    display: grid;
    gap: 0.45rem;
    width: min(100%, 20rem);
  }

  .prism-date-input-label {
    color: var(--prism-color-text-strong);
    font-size: 0.85rem;
    font-weight: 700;
  }

  .prism-date-input-control-wrap {
    position: relative;
    min-width: 0;
  }

  .prism-date-input-control-row {
    display: flex;
    align-items: stretch;
    min-width: 0;
  }

  .prism-date-input-control {
    box-sizing: border-box;
    min-width: 0;
    width: auto;
    flex: 1;
    min-height: 2.65rem;
    padding: 0.55rem 0.7rem;
    border: 1px solid var(--prism-color-border-strong);
    border-radius: var(--prism-radius-medium) 0 0 var(--prism-radius-medium);
    color: var(--prism-color-text-strong);
    background: var(--prism-color-surface-raised);
    font: inherit;
    color-scheme: inherit;
    accent-color: var(--prism-color-action);
    appearance: none;
  }

  .prism-date-input-trigger {
    display: inline-grid;
    flex: 0 0 2.65rem;
    place-items: center;
    min-height: 2.65rem;
    margin-left: -1px;
    padding: 0;
    border: 1px solid var(--prism-color-border-strong);
    border-radius: 0 var(--prism-radius-medium) var(--prism-radius-medium) 0;
    color: var(--prism-color-action);
    background: var(--prism-color-surface-raised);
    cursor: pointer;
  }

  .prism-date-input-trigger:hover,
  .prism-date-input-trigger:focus-visible {
    border-color: var(--prism-color-accent);
    color: var(--prism-color-accent);
    background: var(--prism-color-surface-hover);
  }

  .prism-date-input-trigger:focus-visible {
    position: relative;
    z-index: 1;
    outline: 2px solid var(--prism-color-focus);
    outline-offset: 2px;
  }

  .prism-date-input-trigger:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .prism-theme-model-prism .prism-date-input-control,
  .prism-theme-model-aurora .prism-date-input-control,
  .prism-theme-model-editorial .prism-date-input-control {
    color-scheme: light;
  }

  .prism-theme-model-nocturne .prism-date-input-control,
  .prism-theme-model-terminal .prism-date-input-control {
    color-scheme: dark;
  }

  .prism-date-input-control:hover {
    border-color: var(--prism-color-accent);
  }

  .prism-date-input-control:focus-visible {
    outline: 2px solid var(--prism-color-focus);
    outline-offset: 2px;
  }

  .prism-date-input-control:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .prism-date-picker-small .prism-date-input-control,
  .prism-date-time-picker-small .prism-date-input-control {
    min-height: 2.25rem;
    padding: 0.4rem 0.6rem;
    font-size: 0.85rem;
  }

  .prism-date-picker-small .prism-date-input-trigger,
  .prism-date-time-picker-small .prism-date-input-trigger {
    flex-basis: 2.25rem;
    min-height: 2.25rem;
  }

  .prism-date-picker-large .prism-date-input-control,
  .prism-date-time-picker-large .prism-date-input-control {
    min-height: 3rem;
    padding: 0.7rem 0.8rem;
  }

  .prism-date-picker-large .prism-date-input-trigger,
  .prism-date-time-picker-large .prism-date-input-trigger {
    flex-basis: 3rem;
    min-height: 3rem;
  }

  .prism-date-picker-popup-wrap {
    position: static;
  }

  .prism-date-picker-popup {
    position: absolute;
    z-index: 100;
    top: calc(100% + 0.5rem);
    left: 0;
    width: min(22rem, calc(100vw - 2rem));
    padding: 0.8rem;
    overflow: hidden;
    border: 1px solid var(--prism-color-border);
    border-radius: var(--prism-radius-medium);
    color: var(--prism-color-text-strong);
    background: var(--prism-color-surface-raised);
    box-shadow: var(--prism-shadow-floating);
    color-scheme: inherit;
  }

  .prism-date-picker-popup[hidden] {
    display: none;
  }

  .prism-date-picker-popup-header,
  .prism-date-picker-popup-footer,
  .prism-date-picker-popup-actions {
    display: flex;
    align-items: center;
  }

  .prism-date-picker-popup-header {
    justify-content: space-between;
    gap: 0.5rem;
    margin-bottom: 0.7rem;
  }

  .prism-date-picker-month {
    min-width: 0;
    color: var(--prism-color-text-strong);
    font-size: 0.9rem;
    text-align: center;
  }

  .prism-date-picker-nav,
  .prism-date-picker-today,
  .prism-date-picker-close,
  .prism-date-picker-confirm {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 0.3rem;
    border: 1px solid transparent;
    border-radius: var(--prism-radius-small);
    color: var(--prism-color-action);
    background: transparent;
    font: inherit;
    cursor: pointer;
  }

  .prism-date-picker-nav {
    width: 2rem;
    height: 2rem;
    padding: 0;
  }

  .prism-date-picker-nav:hover,
  .prism-date-picker-nav:focus-visible,
  .prism-date-picker-today:hover,
  .prism-date-picker-today:focus-visible,
  .prism-date-picker-close:hover,
  .prism-date-picker-close:focus-visible {
    border-color: var(--prism-color-border);
    background: var(--prism-color-surface-hover);
    outline: none;
  }

  .prism-date-picker-weekdays,
  .prism-date-picker-grid {
    display: grid;
    grid-template-columns: repeat(7, minmax(0, 1fr));
    gap: 0.2rem;
  }

  .prism-date-picker-weekdays {
    margin-bottom: 0.25rem;
  }

  .prism-date-picker-weekdays span {
    color: var(--prism-color-text-muted);
    font-size: 0.68rem;
    font-weight: 750;
    text-align: center;
  }

  .prism-date-picker-day {
    display: grid;
    min-width: 0;
    min-height: 2rem;
    place-items: center;
    padding: 0.15rem;
    border: 1px solid transparent;
    border-radius: var(--prism-radius-small);
    color: var(--prism-color-text-strong);
    background: transparent;
    font: inherit;
    font-size: 0.78rem;
    cursor: pointer;
  }

  .prism-date-picker-day:hover,
  .prism-date-picker-day:focus-visible {
    border-color: var(--prism-color-accent);
    background: var(--prism-color-surface-hover);
    outline: none;
  }

  .prism-date-picker-day-outside {
    color: var(--prism-color-text-subtle);
  }

  .prism-date-picker-day-today {
    border-color: var(--prism-color-border-strong);
  }

  .prism-date-picker-day-selected {
    border-color: var(--prism-color-action);
    color: var(--prism-color-text-inverse);
    background: var(--prism-color-action);
  }

  .prism-date-picker-day-selected:hover,
  .prism-date-picker-day-selected:focus-visible {
    border-color: var(--prism-color-action-hover);
    color: var(--prism-color-text-inverse);
    background: var(--prism-color-action-hover);
  }

  .prism-date-picker-day:disabled {
    opacity: 0.35;
    cursor: not-allowed;
  }

  .prism-date-picker-time {
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    align-items: end;
    gap: 0.45rem;
    margin-top: 0.75rem;
    padding-top: 0.7rem;
    border-top: 1px solid var(--prism-color-border);
  }

  .prism-date-picker-time-heading {
    grid-column: 1 / -1;
    color: var(--prism-color-text-muted);
    font-size: 0.72rem;
    font-weight: 750;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .prism-date-picker-time-field {
    display: grid;
    gap: 0.25rem;
    color: var(--prism-color-text-muted);
    font-size: 0.7rem;
    font-weight: 650;
  }

  .prism-date-picker-time-field select {
    min-height: 2.1rem;
    padding: 0.35rem 0.5rem;
    border: 1px solid var(--prism-color-border-input);
    border-radius: var(--prism-radius-small);
    color: var(--prism-color-text-strong);
    background: var(--prism-color-surface);
    font: inherit;
    color-scheme: inherit;
  }

  .prism-date-picker-time-separator {
    padding-bottom: 0.45rem;
    color: var(--prism-color-text-muted);
    font-weight: 800;
  }

  .prism-date-picker-popup-footer {
    justify-content: space-between;
    gap: 0.7rem;
    margin-top: 0.75rem;
    padding-top: 0.7rem;
    border-top: 1px solid var(--prism-color-border);
  }

  .prism-date-picker-selected {
    min-width: 0;
    overflow: hidden;
    color: var(--prism-color-text-muted);
    font-size: 0.72rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .prism-date-picker-popup-actions {
    flex: 0 0 auto;
    gap: 0.35rem;
  }

  .prism-date-picker-today,
  .prism-date-picker-close,
  .prism-date-picker-confirm {
    min-height: 2rem;
    padding: 0.35rem 0.5rem;
    font-size: 0.72rem;
    font-weight: 700;
  }

  .prism-date-picker-confirm {
    color: var(--prism-color-text-inverse);
    background: var(--prism-color-action);
  }

  .prism-date-picker-confirm:hover,
  .prism-date-picker-confirm:focus-visible {
    background: var(--prism-color-action-hover);
    outline: none;
  }

  .prism-date-picker-confirm:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .prism-alert {
    display: flex;
    align-items: flex-start;
    gap: 0.7rem;
    min-width: 0;
    padding: 0.8rem 0.9rem;
    border: 1px solid var(--prism-color-border);
    border-radius: var(--prism-radius-medium);
    background: var(--prism-color-surface-raised);
    color: var(--prism-color-text);
  }

  .prism-alert-success { border-color: color-mix(in srgb, var(--prism-color-success) 45%, var(--prism-color-border)); }
  .prism-alert-info { border-color: color-mix(in srgb, var(--prism-color-information) 45%, var(--prism-color-border)); }
  .prism-alert-warning { border-color: color-mix(in srgb, var(--prism-color-warning-end) 60%, var(--prism-color-border)); }
  .prism-alert-error { border-color: color-mix(in srgb, var(--prism-color-danger) 55%, var(--prism-color-border)); }
  .prism-alert-success .prism-alert-icon { color: var(--prism-color-success); }
  .prism-alert-info .prism-alert-icon { color: var(--prism-color-information); }
  .prism-alert-warning .prism-alert-icon { color: var(--prism-color-warning-end); }
  .prism-alert-error .prism-alert-icon { color: var(--prism-color-danger); }
  .prism-alert-icon {
    display: grid;
    flex: 0 0 1.35rem;
    place-items: center;
    width: 1.35rem;
    height: 1.35rem;
    border: 1px solid currentColor;
    border-radius: 50%;
    font-size: 0.78rem;
    font-weight: 800;
  }
  .prism-alert-body { min-width: 0; flex: 1; }
  .prism-alert-title { font-weight: 750; }
  .prism-alert-description { margin-top: 0.2rem; color: var(--prism-color-text-muted); line-height: 1.45; }
  .prism-alert-dismiss,
  .prism-tag-remove {
    display: inline-grid;
    flex: 0 0 auto;
    place-items: center;
    border: 0;
    padding: 0.25rem;
    border-radius: var(--prism-radius-small);
    background: transparent;
    color: inherit;
    cursor: pointer;
  }
  .prism-alert-dismiss:hover,
  .prism-alert-dismiss:focus-visible,
  .prism-tag-remove:hover,
  .prism-tag-remove:focus-visible { background: var(--prism-color-surface-hover); }

  .prism-toast-region {
    position: fixed;
    z-index: 1000;
    display: grid;
    gap: 0.65rem;
    width: min(26rem, calc(100vw - 2rem));
    pointer-events: none;
  }
  .prism-toast-region-top-start { top: 1rem; left: 1rem; }
  .prism-toast-region-top-end { top: 1rem; right: 1rem; }
  .prism-toast-region-bottom-start { bottom: 1rem; left: 1rem; }
  .prism-toast-region-bottom-end { right: 1rem; bottom: 1rem; }
  .prism-toast { pointer-events: auto; animation: prism-toast-in 180ms ease-out both; }
  @keyframes prism-toast-in { from { opacity: 0; transform: translateY(0.5rem); } to { opacity: 1; transform: translateY(0); } }

  .prism-dropdown,
  .prism-popover-anchor,
  .prism-tooltip-anchor { position: relative; display: inline-flex; }
  .prism-dropdown-trigger {
    display: inline-flex;
    align-items: center;
    gap: 0.45rem;
    border: 1px solid var(--prism-color-border);
    border-radius: var(--prism-radius-small);
    padding: 0.55rem 0.7rem;
    background: var(--prism-color-surface-raised);
    color: var(--prism-color-text-strong);
    cursor: pointer;
  }
  .prism-dropdown-trigger:hover { background: var(--prism-color-surface-hover); }
  .prism-dropdown-panel,
  .prism-popover,
  .prism-tooltip {
    z-index: 100;
    border: 1px solid var(--prism-color-border);
    border-radius: var(--prism-radius-medium);
    background: var(--prism-color-surface-raised);
    box-shadow: var(--prism-shadow-floating);
  }
  .prism-dropdown-panel {
    position: fixed;
    min-width: 12rem;
    padding: 0.35rem;
  }
  .prism-dropdown-panel[hidden] {
    display: none;
  }

  .prism-menu { min-width: 11rem; padding: 0.25rem; outline: none; }
  .prism-menu-item {
    display: flex;
    width: 100%;
    align-items: center;
    gap: 0.6rem;
    border: 0;
    border-radius: var(--prism-radius-small);
    padding: 0.55rem 0.65rem;
    background: transparent;
    color: var(--prism-color-text);
    font: inherit;
    text-align: left;
    text-decoration: none;
    cursor: pointer;
  }
  .prism-menu-item:hover,
  .prism-menu-item:focus-visible { background: var(--prism-color-surface-hover); color: var(--prism-color-text-strong); outline: none; }
  .prism-menu-item:disabled,
  .prism-menu-item[aria-disabled="true"] { opacity: 0.45; cursor: not-allowed; }
  .prism-menu-item-icon,
  .prism-menu-item-end { display: inline-grid; place-items: center; flex: 0 0 auto; }
  .prism-menu-item-label { min-width: 0; flex: 1; }
  .prism-menu-item-shortcut { color: var(--prism-color-text-muted); font-size: 0.75rem; }
  .prism-menu-separator { height: 1px; margin: 0.35rem 0; background: var(--prism-color-border); }
  .prism-menu-group-label { padding: 0.5rem 0.65rem 0.25rem; color: var(--prism-color-text-muted); font-size: 0.72rem; font-weight: 750; text-transform: uppercase; letter-spacing: 0.06em; }
  .prism-menu-submenu { position: relative; }
  .prism-menu-submenu > .prism-menu { position: absolute; top: -0.3rem; left: calc(100% + 0.3rem); border: 1px solid var(--prism-color-border); border-radius: var(--prism-radius-medium); background: var(--prism-color-surface-raised); box-shadow: var(--prism-shadow-floating); }

  .prism-tooltip {
    position: fixed;
    top: 0;
    left: 0;
    width: max-content;
    max-width: min(20rem, calc(100vw - 1rem));
    padding: 0.45rem 0.6rem;
    color: var(--prism-color-page);
    background: var(--prism-color-ink);
    font-size: 0.75rem;
    line-height: 1.35;
    pointer-events: none;
  }

  .prism-popover {
    position: fixed;
    min-width: 12rem;
    max-width: min(28rem, calc(100vw - 1rem));
    padding: 1rem;
  }
  .prism-popover-trigger { display: inline-flex; cursor: pointer; }

  .prism-tabs-list { display: flex; gap: 0.2rem; border-bottom: 1px solid var(--prism-color-border); overflow-x: auto; }
  .prism-tabs-vertical { display: grid; grid-template-columns: auto 1fr; gap: 1rem; }
  .prism-tabs-vertical .prism-tabs-list { flex-direction: column; border-right: 1px solid var(--prism-color-border); border-bottom: 0; }
  .prism-tabs-tab {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    border: 0;
    border-bottom: 2px solid transparent;
    padding: 0.65rem 0.8rem;
    background: transparent;
    color: var(--prism-color-text-muted);
    font: inherit;
    white-space: nowrap;
    cursor: pointer;
  }
  .prism-tabs-tab:hover,
  .prism-tabs-tab.is-active { border-bottom-color: var(--prism-color-accent); color: var(--prism-color-text-strong); }
  .prism-tabs-vertical .prism-tabs-tab { justify-content: flex-start; border-right: 2px solid transparent; border-bottom: 0; }
  .prism-tabs-vertical .prism-tabs-tab.is-active { border-right-color: var(--prism-color-accent); }
  .prism-tabs-tab:disabled { opacity: 0.45; cursor: not-allowed; }
  .prism-tabs-panels { padding-top: 1rem; }
  .prism-tabs-panel:focus-visible { outline: 2px solid var(--prism-color-focus); outline-offset: 3px; }

  .prism-progress { display: grid; gap: 0.4rem; width: 100%; }
  .prism-progress-header { display: flex; justify-content: space-between; gap: 1rem; color: var(--prism-color-text-muted); font-size: 0.78rem; }
  .prism-progress-track { position: relative; overflow: hidden; border-radius: 999px; background: var(--prism-color-surface-hover); }
  .prism-progress-small .prism-progress-track { height: 0.25rem; }
  .prism-progress-medium .prism-progress-track { height: 0.45rem; }
  .prism-progress-large .prism-progress-track { height: 0.7rem; }
  .prism-progress-bar { height: 100%; border-radius: inherit; background: var(--prism-color-accent); transition: width 180ms ease; }
  .prism-progress-success .prism-progress-bar { background: var(--prism-color-success); }
  .prism-progress-warning .prism-progress-bar { background: var(--prism-color-warning); }
  .prism-progress-error .prism-progress-bar { background: var(--prism-color-danger); }
  .prism-progress-bar.is-indeterminate { animation: prism-progress-slide 1.2s ease-in-out infinite; }
  @keyframes prism-progress-slide { 0% { transform: translateX(-100%); } 100% { transform: translateX(300%); } }
  .prism-spinner { display: inline-grid; place-items: center; vertical-align: middle; }
  .prism-spinner-small { width: 1rem; height: 1rem; }
  .prism-spinner-medium { width: 1.35rem; height: 1.35rem; }
  .prism-spinner-large { width: 2rem; height: 2rem; }
  .prism-spinner-ring { width: 70%; height: 70%; border: 2px solid color-mix(in srgb, currentColor 20%, transparent); border-top-color: currentColor; border-radius: 50%; color: var(--prism-color-accent); animation: prism-spin 700ms linear infinite; }
  .prism-spinner-success .prism-spinner-ring { color: var(--prism-color-success); }
  .prism-spinner-warning .prism-spinner-ring { color: var(--prism-color-warning); }
  .prism-spinner-error .prism-spinner-ring { color: var(--prism-color-danger); }
  @keyframes prism-spin { to { transform: rotate(360deg); } }
  .prism-skeleton {
    display: block;
    min-height: 0.9rem;
    overflow: hidden;
    background: linear-gradient(
      90deg,
      color-mix(in srgb, var(--prism-color-ink) 16%, var(--prism-color-surface)),
      color-mix(in srgb, var(--prism-color-ink) 6%, var(--prism-color-surface)),
      color-mix(in srgb, var(--prism-color-ink) 16%, var(--prism-color-surface))
    );
    background-size: 200% 100%;
    animation: prism-skeleton-shimmer 1.4s ease-in-out infinite;
  }
  .prism-skeleton-circle {
    flex: 0 0 auto;
    aspect-ratio: 1;
    min-width: 2.25rem;
    min-height: 2.25rem;
    border-radius: 50%;
  }
  .prism-skeleton-rect {
    min-height: 2.75rem;
    border-radius: var(--prism-radius-small);
  }
  .prism-skeleton-radius-small { border-radius: var(--prism-radius-small); }
  .prism-skeleton-radius-medium { border-radius: var(--prism-radius-medium); }
  .prism-skeleton-radius-pill { border-radius: 999px; }
  @keyframes prism-skeleton-shimmer { 0% { background-position: 100% 0; } 100% { background-position: -100% 0; } }

  .prism-empty-state { display: grid; justify-items: center; gap: 0.55rem; padding: 3rem 1.5rem; color: var(--prism-color-text-muted); text-align: center; }
  .prism-empty-state h3 { margin: 0; color: var(--prism-color-text-strong); font-size: 1.05rem; }
  .prism-empty-state p { max-width: 34rem; margin: 0; line-height: 1.5; }
  .prism-empty-state-icon { display: grid; width: 3rem; height: 3rem; place-items: center; border-radius: 50%; background: var(--prism-color-surface-hover); color: var(--prism-color-accent); font-size: 1.35rem; }
  .prism-empty-state-actions { display: flex; flex-wrap: wrap; justify-content: center; gap: 0.6rem; margin-top: 0.55rem; }
  .prism-empty-state-error .prism-empty-state-icon { color: var(--prism-color-danger); }
  .prism-pagination { display: flex; align-items: center; justify-content: space-between; gap: 1rem; flex-wrap: wrap; }
  .prism-pagination-pages { display: flex; align-items: center; gap: 0.25rem; }
  .prism-pagination-button { display: inline-grid; min-width: 2rem; height: 2rem; place-items: center; border: 1px solid transparent; border-radius: var(--prism-radius-small); background: transparent; color: var(--prism-color-text); cursor: pointer; }
  .prism-pagination-button:hover:not(:disabled),
  .prism-pagination-button.is-active { border-color: var(--prism-color-accent); background: var(--prism-color-surface-hover); color: var(--prism-color-text-strong); }
  .prism-pagination-button:disabled { opacity: 0.4; cursor: not-allowed; }
  .prism-pagination-ellipsis { min-width: 1.5rem; color: var(--prism-color-text-muted); text-align: center; }
  .prism-pagination-size { display: inline-flex; align-items: center; gap: 0.5rem; color: var(--prism-color-text-muted); font-size: 0.8rem; }
  .prism-pagination-size select { border: 1px solid var(--prism-color-border); border-radius: var(--prism-radius-small); padding: 0.35rem; background: var(--prism-color-surface-raised); color: inherit; }

  .prism-avatar { position: relative; display: inline-grid; flex: 0 0 auto; place-items: center; overflow: visible; background: var(--prism-color-accent); color: var(--prism-color-text-inverse); font-weight: 750; }
  .prism-avatar-circle { border-radius: 50%; }
  .prism-avatar-square { border-radius: var(--prism-radius-small); }
  .prism-avatar-small { width: 1.75rem; height: 1.75rem; font-size: 0.65rem; }
  .prism-avatar-medium { width: 2.5rem; height: 2.5rem; font-size: 0.8rem; }
  .prism-avatar-large { width: 3.5rem; height: 3.5rem; font-size: 1rem; }
  .prism-avatar img { width: 100%; height: 100%; object-fit: cover; border-radius: inherit; }
  .prism-avatar-status { position: absolute; right: -0.05rem; bottom: -0.05rem; width: 0.72rem; height: 0.72rem; border: 2px solid var(--prism-color-surface-raised); border-radius: 50%; background: var(--prism-color-text-muted); }
  .prism-avatar-status-size-small { width: 0.58rem; height: 0.58rem; }
  .prism-avatar-status-size-medium { width: 0.72rem; height: 0.72rem; }
  .prism-avatar-status-size-large { width: 0.86rem; height: 0.86rem; }
  .prism-avatar-status-online { background: var(--prism-color-success); }
  .prism-avatar-status-away { background: var(--prism-color-warning); }
  .prism-avatar-status-offline { background: var(--prism-color-text-muted); }
  .prism-tag { display: inline-flex; align-items: center; gap: 0.25rem; max-width: 100%; border: 1px solid var(--prism-color-border); border-radius: 999px; padding: 0.25rem 0.55rem; background: var(--prism-color-surface-hover); color: var(--prism-color-text-strong); font-size: 0.75rem; font-weight: 650; }
  .prism-tag-icon { display: inline-grid; flex: 0 0 auto; place-items: center; line-height: 1; }
  .prism-tag-success { border-color: color-mix(in srgb, var(--prism-color-success) 45%, var(--prism-color-border)); color: var(--prism-color-success); }
  .prism-tag-warning { border-color: color-mix(in srgb, var(--prism-color-warning) 45%, var(--prism-color-border)); color: var(--prism-color-warning); }
  .prism-tag-error { border-color: color-mix(in srgb, var(--prism-color-danger) 45%, var(--prism-color-border)); color: var(--prism-color-danger); }
  .prism-separator { display: flex; align-items: center; gap: 0.65rem; color: var(--prism-color-text-muted); }
  .prism-separator-horizontal { width: 100%; min-height: 1px; background: var(--prism-color-border); }
  .prism-separator-horizontal:has(span) { background: none; }
  .prism-separator-horizontal:has(span)::before,
  .prism-separator-horizontal:has(span)::after { content: ''; flex: 1; height: 1px; background: var(--prism-color-border); }
  .prism-separator-vertical { width: 1px; min-height: 1rem; background: var(--prism-color-border); }
  .prism-separator span { font-size: 0.72rem; }
  .prism-stack { display: flex; }
  .prism-stack-column { flex-direction: column; }
  .prism-stack-row { flex-direction: row; }
  .prism-stack-wrap { flex-wrap: wrap; }
  .prism-stack-gap-none { gap: 0; }
  .prism-stack-gap-small { gap: 0.5rem; }
  .prism-stack-gap-medium { gap: 1rem; }
  .prism-stack-gap-large { gap: 1.5rem; }
  .prism-grid { --prism-grid-gap: 1rem; display: grid; gap: var(--prism-grid-gap); }
  .prism-grid-gap-none { --prism-grid-gap: 0; }
  .prism-grid-gap-small { --prism-grid-gap: 0.5rem; }
  .prism-grid-gap-medium { --prism-grid-gap: 1rem; }
  .prism-grid-gap-large { --prism-grid-gap: 1.5rem; }

  @media (prefers-reduced-motion: reduce) {
    .prism-toast,
    .prism-progress-bar,
    .prism-spinner-ring,
    .prism-skeleton { animation: none; transition: none; }
  }

  @media (forced-colors: active) {
    .prism-background {
      border-color: CanvasText;
      color: CanvasText;
      background: Canvas;
      box-shadow: none;
    }

    .prism-background-canvas,
    .prism-background-wash {
      display: none;
    }

    .prism-button,
    .prism-select-trigger,
    .prism-select-option,
    .prism-auto-complete-input,
    .prism-auto-complete-option,
    .prism-auto-complete-menu,
    .prism-popup-close,
    .prism-table button,
    .prism-table select,
    .prism-table input,
    .text-field,
    .check-box-input {
      forced-color-adjust: auto;
    }

    .prism-button {
      border-color: ButtonText;
      color: ButtonText;
      background: ButtonFace;
      box-shadow: none;
    }

    .prism-button:hover:not(:disabled),
    .prism-button:focus-visible,
    .prism-select-option:hover:not(:disabled),
    .prism-select-option[aria-selected="true"],
    .prism-select-option[data-active="true"],
    .prism-auto-complete-option:hover:not(:disabled),
    .prism-auto-complete-option[aria-selected="true"],
    .prism-auto-complete-option[data-active="true"] {
      border-color: Highlight;
      color: HighlightText;
      background: Highlight;
    }

    .prism-button:disabled,
    .prism-select-trigger:disabled {
      color: GrayText;
    }

    .prism-select-trigger,
    .prism-select-option,
    .prism-auto-complete-input,
    .prism-auto-complete-option,
    .prism-auto-complete-menu,
    .prism-popup-close,
    .prism-table select,
    .text-field {
      border-color: ButtonText;
      color: FieldText;
      background: Field;
      box-shadow: none;
    }

    .prism-table,
    .prism-popup-panel,
    .prism-auto-complete-menu,
    .prism-code,
    .prism-code-highlight,
    .prism-code-input,
    .prism-tree-model-nocturne,
    .prism-tree-model-editorial,
    .prism-tree-model-terminal,
    .prism-tree-model-aurora {
      border-color: CanvasText;
      color: CanvasText;
      background: Canvas;
      box-shadow: none;
    }

    .prism-code-highlight,
    .prism-code-input,
    .prism-tree-summary,
    .prism-tree-link,
    .prism-tree-toggle {
      color: CanvasText;
      background: Canvas;
      border-color: CanvasText;
      box-shadow: none;
    }

    .prism-tree-summary-active,
    .prism-tree-link-active,
    .prism-table-row-selected td {
      color: HighlightText;
      background: Highlight;
      border-color: Highlight;
      --prism-table-row-background: Highlight;
    }

    :where(button, input, select, textarea, [tabindex]):focus-visible {
      outline: 2px solid Highlight;
      outline-offset: 2px;
    }
  }
`),tr=`/assets/links-app-icon-DT8Ilv_q.png`,nr=`links-web-client-preview-v1`,rr=O(`/links-api`),ir=O(``),ar=O(`micky`),or=O(`preview`),Y=O(`karine`),sr=O(``),cr=O(``),lr=O(``),X=O(``),ur=O(!1),dr=O(!1),fr=O(!1),pr=O(!1),mr=O(!1),hr={contacts:[{handle:`karine`,userID:`4bc1797c-2dc3-4854-aada-6a52037a35e1`,deviceCount:2},{handle:`bob`,userID:`c394da90-1982-4541-bc6a-af981bd67978`,deviceCount:1}],conversations:[{id:`karine`,title:`@karine`,recipientUserID:`4bc1797c-2dc3-4854-aada-6a52037a35e1`,unreadCount:0,messages:[{id:`m1`,text:`The incoming username now resolves on my side.`,outgoing:!1,sentAt:`09:41`},{id:`m2`,text:`Perfect. I’m checking the web client shell next.`,outgoing:!0,sentAt:`09:43`}]},{id:`bob`,title:`@bob`,recipientUserID:`c394da90-1982-4541-bc6a-af981bd67978`,unreadCount:2,messages:[{id:`m3`,text:`Can you see this conversation?`,outgoing:!1,sentAt:`Yesterday`}]}]};function gr(){try{let e=JSON.parse(localStorage.getItem(nr)||`null`);if(e?.contacts&&e?.conversations)return e}catch{}return hr}var _r=gr(),Z=O(_r.contacts),Q=O(_r.conversations);function vr(){try{localStorage.setItem(nr,JSON.stringify({contacts:Z.value,conversations:Q.value}))}catch{}}function $(e){return String(e||``).trim().toLowerCase().replace(/^@/,``)}function yr(e){return/^[a-z0-9_]{3,32}$/.test(e)}function br(){let e=ir.value.trim();return e?{Authorization:`Bearer ${e}`}:{}}async function xr(e){let t=$(e);if(!yr(t))throw Error(`Use 3–32 lowercase letters, numbers, or underscores.`);if(!ir.value.trim()){let e=Z.value.find(e=>e.handle===t);if(e)return e;throw Error(`Add a bearer token in Profile to resolve a new username.`)}let n=rr.value.trim().replace(/\/$/,``),r=await fetch(`${n}/v1/directory/${encodeURIComponent(t)}`,{headers:br(),cache:`no-store`,credentials:`omit`,redirect:`error`});if(r.status===404)throw Error(`No Links account uses @${t}.`);if(!r.ok)throw Error(`The directory could not resolve that username.`);let i=await r.json();return{handle:$(i.handle||t),userID:i.user_id||i.userID,deviceCount:Array.isArray(i.devices)?i.devices.length:Number(i.device_count||0)}}function Sr(e){Y.value=e,Q.value=Q.value.map(t=>t.id===e?{...t,unreadCount:0}:t),pr.value=!1,vr()}function Cr(e){let t=Q.value.find(t=>t.recipientUserID===e.userID);if(t){Sr(t.id);return}let n={id:crypto.randomUUID(),title:`@${e.handle}`,recipientUserID:e.userID,unreadCount:0,messages:[]};Q.value=[n,...Q.value],Y.value=n.id,vr()}async function wr(e){if(e?.preventDefault(),!mr.value){X.value=``,mr.value=!0;try{let e=await xr(cr.value);if(!e.userID)throw Error(`The directory response did not include a user ID.`);Z.value=[...Z.value.filter(t=>t.userID!==e.userID),e].sort((e,t)=>e.handle.localeCompare(t.handle)),Cr(e),cr.value=``,ur.value=!1,dr.value=!1,X.value=`Opened @${e.handle}.`,vr()}catch(e){X.value=e.message}finally{mr.value=!1}}}function Tr(e){e?.preventDefault();let t=sr.value.trim(),n=Y.value;t&&n&&(Q.value=Q.value.map(e=>e.id===n?{...e,messages:[...e.messages,{id:crypto.randomUUID(),text:t,outgoing:!0,sentAt:new Intl.DateTimeFormat([],{hour:`2-digit`,minute:`2-digit`}).format(new Date)}]}:e),sr.value=``,X.value=`Saved in the local preview. Encrypted transport is not connected yet.`,vr(),requestAnimationFrame(()=>document.querySelector(`.message-list`)?.scrollTo({top:999999,behavior:`smooth`})))}function Er(){Z.value=hr.contacts,Q.value=hr.conversations,Y.value=`karine`,vr()}var Dr=M(()=>Q.value.find(e=>e.id===Y.value)||null),Or=M(()=>{let e=$(lr.value);return Q.value.filter(t=>!e||$(t.title).includes(e))}),kr=M(()=>or.value===`ready`?`Connected`:`UI preview`);function Ar(){return z(`span`,{class:M(()=>`status-dot is-${or.value}`),"aria-hidden":`true`})}function jr(){return M(()=>Or.value.length?Or.value.map(e=>B(`button`,{type:`button`,class:M(()=>`conversation-row ${Y.value===e.id?`is-selected`:``}`),onClick:()=>Sr(e.id),children:[z(Tn,{name:e.title,size:`medium`}),B(`span`,{class:`conversation-copy`,children:[z(`strong`,{children:e.title}),z(`span`,{children:e.messages.at(-1)?.text||`No messages yet`})]}),e.unreadCount>0?z(Dn,{value:e.unreadCount>99?`99+`:e.unreadCount,tone:`info`}):null]})):z(`p`,{class:`sidebar-empty`,children:`No matching conversations.`}))}function Mr(){return M(()=>Z.value.length?Z.value.map(e=>B(`button`,{type:`button`,class:`contact-row`,onClick:()=>Cr(e),children:[z(Tn,{name:e.handle,size:`small`}),B(`span`,{children:[B(`strong`,{children:[`@`,e.handle]}),B(`small`,{children:[e.deviceCount||`No`,` active `,e.deviceCount===1?`device`:`devices`]})]})]})):z(`p`,{class:`sidebar-empty`,children:`Add someone by username.`}))}function Nr(){return B(`aside`,{class:M(()=>`sidebar ${pr.value?`is-open`:``}`),children:[B(`div`,{class:`sidebar-brand`,children:[z(`img`,{class:`brand-mark`,src:tr,alt:``,"aria-hidden":`true`}),B(`span`,{children:[z(`strong`,{children:`Links`}),z(`small`,{children:`Private messaging`})]}),z(G,{label:`Close`,showLabel:!1,icon:z(dn,{}),ariaLabel:`Close sidebar`,variant:`tertiary`,size:`small`,class:`mobile-close`,onClick:()=>{pr.value=!1}})]}),B(`div`,{class:`sidebar-search`,children:[z(fn,{size:`0.9rem`}),z(`input`,{value:lr,onInput:e=>{lr.value=e.currentTarget.value},placeholder:`Search conversations`,"aria-label":`Search conversations`})]}),B(`section`,{class:`sidebar-section conversations-section`,children:[B(`div`,{class:`section-label`,children:[z(`span`,{children:`Conversations`}),z(G,{label:`New conversation`,showLabel:!1,icon:z(un,{}),ariaLabel:`New conversation`,variant:`tertiary`,size:`small`,onClick:()=>{ur.value=!0}})]}),z(`div`,{class:`conversation-list`,children:z(jr,{})})]}),B(`section`,{class:`sidebar-section contacts-section`,children:[B(`div`,{class:`section-label`,children:[z(`span`,{children:`Contacts`}),z(G,{label:`Add contact`,showLabel:!1,icon:z(gn,{}),ariaLabel:`Add contact`,variant:`tertiary`,size:`small`,onClick:()=>{dr.value=!0}})]}),z(`div`,{class:`contact-list`,children:z(Mr,{})})]}),B(`button`,{type:`button`,class:`profile-card`,onClick:()=>{fr.value=!0},children:[z(Tn,{name:ar,size:`medium`,status:`online`}),B(`span`,{class:`profile-copy`,children:[z(`strong`,{children:M(()=>`@${$(ar.value)||`profile`}`)}),B(`span`,{children:[z(Ar,{}),` `,kr]})]}),z(_n,{size:`1rem`})]})]})}function Pr(){return M(()=>{let e=Dr.value;return e?e.messages.length===0?z(Nn,{icon:z(hn,{size:`1.4rem`}),title:`No messages yet`,description:`Messages in this conversation will be end-to-end encrypted once the browser core is connected.`}):e.messages.map(e=>z(`div`,{class:`message-row ${e.outgoing?`is-outgoing`:`is-incoming`}`,children:B(`div`,{class:`message-bubble`,children:[z(`p`,{children:e.text}),z(`time`,{children:e.sentAt})]})})):null})}function Fr(){return M(()=>{let e=Dr.value;return e?B(`main`,{class:`conversation-detail`,children:[B(`header`,{class:`conversation-header`,children:[z(G,{label:`Open sidebar`,showLabel:!1,icon:z(pn,{}),ariaLabel:`Open conversations`,variant:`tertiary`,size:`small`,class:`mobile-menu`,onClick:()=>{pr.value=!0}}),z(Tn,{name:e.title,size:`large`}),B(`div`,{class:`conversation-heading`,children:[z(`h1`,{children:e.title}),z(`p`,{children:`Private one-to-one conversation`})]}),B(`div`,{class:`conversation-status`,children:[z(Ar,{}),z(`span`,{children:kr})]})]}),B(`div`,{class:`delivery-banner`,children:[z(ln,{size:`1rem`}),B(`div`,{children:[z(`strong`,{children:`Browser transport not connected`}),z(`span`,{children:`This shell is ready for the shared WASM messaging core and WebTextMessaging host.`})]}),z(Dn,{value:`Preview`,tone:`warning`})]}),B(`div`,{class:`secure-note`,children:[z(hn,{size:`0.8rem`}),z(`span`,{children:`Secure conversation initialization is pending browser-core integration.`})]}),z(`section`,{class:`message-list`,"aria-live":`polite`,children:z(Pr,{})}),B(`form`,{class:`composer`,onSubmit:Tr,children:[z(`textarea`,{value:sr,onInput:e=>{sr.value=e.currentTarget.value},onKeyDown:e=>{e.key===`Enter`&&!e.shiftKey&&(e.preventDefault(),Tr(e))},placeholder:`Message`,"aria-label":`Message ${e.title}`,rows:`1`}),z(G,{type:`submit`,label:`Send`,showLabel:!1,icon:z(mn,{}),ariaLabel:`Save preview message`,variant:`primary`,disabled:M(()=>!sr.value.trim())})]})]}):z(`main`,{class:`empty-detail`,children:z(Nn,{icon:z(pn,{size:`1.7rem`}),title:`Choose a conversation`,description:`Open a contact or create a conversation to start messaging.`})})})}function Ir({open:e,title:t,description:n}){return z(Kn,{open:e,title:t,ariaDescription:n,size:`small`,onClose:()=>{cr.value=``,X.value=``},footer:()=>B(`div`,{class:`popup-actions`,children:[z(G,{label:`Cancel`,variant:`secondary`,onClick:()=>{e.value=!1}}),z(G,{label:`Find and open`,icon:z(fn,{}),variant:`primary`,loading:mr,onClick:wr})]}),children:B(`form`,{class:`popup-form`,onSubmit:wr,children:[z(`label`,{for:`contact-handle`,children:`Username`}),z(Zn,{id:`contact-handle`,value:cr,placeholder:`alice`,autocomplete:`off`}),z(`p`,{children:`Recipient IDs and device counts resolve automatically through the directory API.`}),M(()=>X.value?z(bn,{tone:`error`,children:X}):null)]})})}function Lr(){return z(Kn,{open:fr,title:`Web profile`,ariaDescription:`Configure the local interface preview and directory access.`,size:`medium`,footer:()=>B(`div`,{class:`popup-actions is-split`,children:[z(G,{label:`Reset preview`,variant:`tertiary`,onClick:Er}),z(G,{label:`Save`,variant:`primary`,onClick:()=>{fr.value=!1,X.value=`Profile settings updated.`}})]}),children:B(`div`,{class:`profile-form`,children:[z(`label`,{for:`profile-handle`,children:`Profile username`}),z(Zn,{id:`profile-handle`,value:ar,placeholder:`username`,autocomplete:`username`}),z(`label`,{for:`auth-base`,children:`Account service`}),z(Zn,{id:`auth-base`,value:rr,placeholder:`/links-api`,autocomplete:`off`}),z(`label`,{for:`access-token`,children:`Bearer token`}),z(Zn,{id:`access-token`,value:ir,type:`password`,placeholder:`Optional for directory lookup`,autocomplete:`off`}),B(`div`,{class:`privacy-copy`,children:[z(hn,{size:`0.9rem`}),z(`span`,{children:`The token stays in memory and is never written to browser storage.`})]})]})})}function Rr(){return B(`div`,{class:`app`,"use:style":er,children:[z(`div`,{class:`mobile-scrim`,onClick:()=>{pr.value=!1}}),z(Nr,{}),z(Fr,{}),z(Ir,{open:ur,title:`New conversation`,description:`Find someone by username and open a private conversation.`}),z(Ir,{open:dr,title:`Add contact`,description:`Resolve and save a Links account by username.`}),z(Lr,{}),M(()=>X.value&&!ur.value&&!dr.value?z(`div`,{class:`toast`,role:`status`,children:X}):null)]})}qt(z(Rr,{}),document.querySelector(`#app`));