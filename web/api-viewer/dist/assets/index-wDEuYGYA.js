(function(){let e=document.createElement(`link`).relList;if(e&&e.supports&&e.supports(`modulepreload`))return;for(let e of document.querySelectorAll(`link[rel="modulepreload"]`))n(e);new MutationObserver(e=>{for(let t of e)if(t.type===`childList`)for(let e of t.addedNodes)e.tagName===`LINK`&&e.rel===`modulepreload`&&n(e)}).observe(document,{childList:!0,subtree:!0});function t(e){let t={};return e.integrity&&(t.integrity=e.integrity),e.referrerPolicy&&(t.referrerPolicy=e.referrerPolicy),t.credentials=e.crossOrigin===`use-credentials`?`include`:e.crossOrigin===`anonymous`?`omit`:`same-origin`,t}function n(e){if(e.ep)return;e.ep=!0;let n=t(e);fetch(e.href,n)}})();var e=Object.freeze({development:!1,bindingWarningThreshold:50});function t(){return e}function n(){return e.development}function r(e,t){if(typeof e!=`string`||e.length===0)return``;let n=e.toLowerCase(),r,a=1/0;for(let e of t){let t=i(n,e.toLowerCase());t<a&&(r=e,a=t)}let o=Math.max(1,Math.floor(n.length/3));return r&&a<=o?` Did you mean "${r}"?`:``}function i(e,t){let n=Array.from({length:t.length+1},(e,t)=>t);for(let r=1;r<=e.length;r+=1){let i=n[0];n[0]=r;for(let a=1;a<=t.length;a+=1){let o=n[a];n[a]=e[r-1]===t[a-1]?i:Math.min(i,n[a-1],o)+1,i=o}}return n[t.length]}var a=new Map([[`renderer`,new Set],[`scheduler`,new Set],[`logger`,new Set],[`style`,new Set]]);function o(e){let t=a.get(e);if(!t)throw TypeError(`Unknown plugin extension point: ${e}${r(e,[...a.keys()])}`);return t}function s(e,t){let n=o(e);if(n.size!==0)for(let e of[...n])e(t)}var c=[`/src/utils/diagnostics.js`,`/src/components/index.js`,`/src/reactivity/context.js`,`/src/reactivity/source.js`,`/src/reactivity/signal.js`,`/src/reactivity/computed.js`,`/src/dom/template.js`,`/src/utils/form.js`];function l(e){if(e===null)return`null`;if(e===void 0)return`undefined`;if(typeof e==`function`)return e.name?`function ${e.name}`:`an anonymous function`;if(typeof e==`object`)try{let t=e.constructor?.name;return t?`an object (${t})`:`an object`}catch{return`an object`}try{return`${typeof e} (${String(e)})`}catch{return typeof e}}function u(e,t={}){try{let n=`[Matrix] ${e}`;t.stack?globalThis.console?.warn?.(`${n}\n${t.stack}`):globalThis.console?.warn?.(n)}catch{}try{s(`logger`,{...t,type:t.type??`warning`,message:e})}catch{}}function d(){let e=Error().stack;return e?e.split(`
`).slice(2).filter(e=>!c.some(t=>e.includes(t))).join(`
`):``}function f(e,t={}){n()&&u(e,t)}var p=null,m=null,h=null,g=[],_=[],v=[],y=new WeakSet;function b(e,t){g.push(p),p=e;try{return t()}finally{p=g.pop()}}function x(e){if(!p){n()&&!h&&!y.has(e)&&(y.add(e),f(`${e.kind} "${e.name||`anonymous`}" was read outside an Effect or template. Use .peek() for an intentional non-reactive read.`,{type:`reactivity:untracked-read`,kind:e.kind,name:e.name,source:e,stack:d()}));return}p!==e&&(e.subscribers.add(p),p.dependencies.add(e))}function S(){return m}function C(e,t){_.push(m),m=e;try{return t()}finally{m=_.pop()}}function w(){return h}function T(e,t){v.push(h),h=e;try{return t()}finally{h=v.pop()}}var E=1,D=new Set;function O(e,t=``){let n={id:`source-${E++}`,kind:e,name:t,subscribers:new Set,listeners:new Set};return D.add(n),n}function ee(e){D.delete(e)}function te(e){x(e)}function ne(e,t,n){let r;for(let t of[...e.subscribers])try{t._notify()}catch(e){r??=e}for(let i of[...e.listeners])try{i(t,n)}catch(e){r??=e}if(r)throw r}function re(e,t){if(typeof t!=`function`)throw TypeError(`signal.subscribe() expects a function`);return e.listeners.add(t),()=>{e.listeners.delete(t)}}function k(e,t={}){let n=w();if(n?.isRendering){let r=n.stateCursor;n.stateCursor+=1;let i=n.stateSlots[r];if(i){if(i.kind!==`signal`)throw Error(`Component state order changed at slot ${r}`);return i.value}let a=n.stateScope?n.stateScope.run(()=>A(e,t)):A(e,t);return n.stateSlots[r]={kind:`signal`,value:a},a}return A(e,t)}function A(e,t){let n=t.equals??Object.is,r=O(`signal`,t.name??``),i=e,a=!1,o={get value(){if(a)throw Error(`Cannot read a disposed signal`);return te(r),i},set value(e){if(a)throw Error(`Cannot write to a disposed signal`);if(n(i,e))return;let t=i;i=e,ne(r,i,t)},get(){return o.value},set(e){return o.value=e,i},update(e){if(typeof e!=`function`)throw TypeError(`signal.update() expects a function`);return o.value=e(i),i},peek(){if(a)throw Error(`Cannot read a disposed signal`);return i},dispose(){a||(a=!0,r.subscribers.clear(),r.listeners.clear(),ee(r))},get name(){return t.name??``},subscribe(e){return re(r,e)},get kind(){return r.kind},_source:r};r.read=o.peek;let s=S();return s&&s.add(o.dispose),o}var j=0,M=!1,N=!1,ie=new Set;function P(){return j>0}function ae(){if(N)return;N=!0;let e=()=>{N=!1,se()};typeof queueMicrotask==`function`?queueMicrotask(e):Promise.resolve().then(e)}function oe(e,t=`sync`){if(s(`scheduler`,{type:`job:scheduled`,flush:t}),t===`microtask`||j>0){ie.add(e),(j===0||t===`microtask`)&&ae();return}e()}function se(){if(M||j>0)return;M=!0,s(`scheduler`,{type:`flush:start`,size:ie.size});let e;try{for(;ie.size>0;){let t=[...ie];ie.clear();for(let n of t)try{n()}catch(t){e??=t}}}finally{M=!1,s(`scheduler`,{type:`flush:end`})}if(e)throw e}var ce=100,le=new Set,ue=1;function de(e,t={}){if(typeof e!=`function`)throw TypeError(`effect() expects a function`);let n=t.flush??`sync`;if(n!==`sync`&&n!==`microtask`)throw TypeError(`effect() accepts flush: 'sync' or 'microtask'. Received ${l(n)}${r(n,[`sync`,`microtask`])}`);let i=new Set,a=typeof t.onError==`function`?t.onError:null,o=t.warnOnDependencyChange??!0,s=new Set,c=!1,d=!1,f=!1,p,m=!1,h=!1,g=!1,_=!1,v={id:`effect-${ue++}`,kind:`effect`,name:t.name??``,dependencies:i,_notify(){if(!g){if(m){h=!0;return}w()}}};function y(){for(let e of i)e.subscribers.delete(v);i.clear()}function x(){if(typeof p!=`function`)return;let e=p;p=void 0,e()}function C(){if(g||m)return;m=!0;let t=0;try{do{if(h=!1,t+=1,t>ce)throw Error(`Reactive loop detected in effect()`);let n=typeof p==`function`;x(),y();let r=b(v,e),a=!!(r&&typeof r.then==`function`);if(a&&!f&&(f=!0,u(`Effect "${v.name||`anonymous`}" returned a Promise. Matrix does not await Effects; cancel async work in cleanup or use resource() to avoid stale closures.`,{type:`effect:async-return`,name:v.name,source:v})),c){let e=[...i].filter(e=>!s.has(e)),t=[...s].filter(e=>!i.has(e));(e.length>0||t.length>0)&&o&&u(`Effect "${v.name||`anonymous`}" changed dependencies (${e.length} added, ${t.length} removed). Return cleanup to cancel work from the previous run and prevent stale closures.`,{type:`effect:dependencies-changed`,name:v.name,added:e.length,removed:t.length,staleClosureRisk:d||!n,source:v})}s=new Set(i),d=a,c=!0,typeof r==`function`&&(p=r)}while(h&&!g)}catch(e){try{T()}catch{}if(a){a(e);return}throw e}finally{m=!1}}function w(){_||g||(_=!0,oe(()=>{_=!1,C()},n))}function T(){if(g)return;g=!0,_=!1;let e;try{x()}catch(t){e=t}try{y()}catch(t){e??=t}if(D&&D(),le.delete(v),e)throw e}let E=S(),D=E?E.add(T):null;try{le.add(v),C()}catch(e){throw T(),e}return T}function F(e,t={}){let n=w();if(n?.isRendering){let r=n.stateCursor;n.stateCursor+=1;let i=n.stateSlots[r];if(i){if(i.kind!==`computed`)throw Error(`Component state order changed at slot ${r}`);return i.value}let a=n.stateScope?n.stateScope.run(()=>fe(e,t)):fe(e,t);return n.stateSlots[r]={kind:`computed`,value:a},a}return fe(e,t)}function fe(e,t){let n=typeof e==`function`?e:e?.get,r=e&&typeof e==`object`?e.set:void 0;if(typeof n!=`function`)throw TypeError(`computed() expects a function`);if(r!==void 0&&typeof r!=`function`)throw TypeError(`computed() expects a valid setter`);let i=t.equals??Object.is,a=O(`computed`,t.name??``),o=new Set,s,c=!0,l=!1,u=!1,d=!1;function f(){if(d=!1,u||!c||a.subscribers.size===0&&a.listeners.size===0)return;let e=s;h(),i(e,s)||ne(a,s,e)}let p={id:`computed-${a.id}`,kind:`computed`,dependencies:o,_notify(){if(!(u||c)&&(c=!0,a.subscribers.size!==0||a.listeners.size!==0)){if(P()){d||(d=!0,oe(f));return}f()}}};function m(){for(let e of o)e.subscribers.delete(p);o.clear()}function h(){if(!c||u)return s;if(l)throw Error(`Reactive loop detected in computed()`);l=!0,m();try{s=b(p,n),c=!1}finally{l=!1}return s}let g={get value(){if(u)throw Error(`Cannot read a disposed computed value`);return te(a),h()},get(){return g.value},set value(e){if(!r)throw TypeError(`This computed value is read-only`);r(e)},set(e){return g.value=e,g.value},peek(){return h()},subscribe(e){return re(a,e)},get kind(){return a.kind},get name(){return t.name??``},_source:a};function _(){u||(u=!0,m(),a.subscribers.clear(),a.listeners.clear(),ee(a))}g.dispose=_,a.read=g.peek;let v=S();return v&&v.add(_),g}function pe(e){typeof e==`function`&&e()}function me(e=S()){let t=new Set,n=new Set,r=!1,i={get disposed(){return r},run(e){if(r)throw Error(`Cannot use a disposed scope`);return C(i,e)},add(e){if(typeof e!=`function`)throw TypeError(`A cleanup must be a function`);return r?(pe(e),()=>{}):(n.add(e),()=>{n.delete(e)})},dispose(){if(r)return;r=!0;let a;for(let e of[...t])try{e.dispose()}catch(e){a??=e}t.clear();for(let e of[...n]){n.delete(e);try{pe(e)}catch(e){a??=e}}if(e&&e._children.delete(i),a)throw a},_children:t};return e&&e._children.add(i),i}var I=null,he=[],ge=Symbol(`matrix.error.boundary`);function L(e,t){he.push(I),I=e;try{return t()}finally{I=he.pop()}}function _e(){return I}var ve=Symbol(`matrix.component.result`);function ye(e,t,n){let r=String(n);f(`Component "${e||`anonymous`}" props are read-only. Cannot ${t} "${r}". Update the owner state instead.`,{type:`component:prop-mutation`,name:e||`anonymous`,operation:t,property:r,stack:d()})}function be(e,t){return new Proxy(e,{set(e,n){throw ye(t,`set`,n),TypeError(`Component props are read-only`)},deleteProperty(e,n){throw ye(t,`delete`,n),TypeError(`Component props are read-only`)}})}function xe(e,t={},n){if(typeof e!=`function`)throw TypeError(`component() expects a render function. Received ${l(e)}. Pass a function such as component(props => html\`<div>...</div>\`).`);let r=be(t&&typeof t==`object`?t:{},e.name),i={[ve]:!0,key:n,render:e,props:r,update(t){return t?.render===e&&t?.key===n}};return Object.defineProperty(i,"_matrixSourceLocation",{value:d(),enumerable:!1}),i}function Se(e){return!!(e&&e[ve])}function Ce(e){if(typeof e!=`function`)throw TypeError(`onMount() expects a function`);let t=_e();if(!t)throw Error(`onMount() must be called inside a component`);t.isMounted||t.mountCallbacks.push(e)}var we=new WeakMap,Te=/__MATRIX_ATTR_(\d+)__/g,Ee=/^matrix:text:(\d+)$/,De=/(?:^|[>\s])\{\s*([A-Za-z_$][\w$]*(?:\s*\.\s*[A-Za-z_$][\w$]*)*)\s*\}(?=\s|<|$)/,Oe=/\\\$\{\s*([^}]+)\}/,ke=Symbol(`matrix.template.result`);function R(e,...t){if(!Array.isArray(e)||!Array.isArray(e.raw))throw TypeError(`html() must be used as a tagged template`);return Ae(e),{[ke]:!0,strings:e,values:t}}function Ae(e){let t=e.raw.join(``),n=Oe.exec(t);if(n){let e=n[1].trim(),t="${"+e+`}`;f(`Template contains an escaped interpolation "${"\\${"+e+`}`}". Did you mean "${t}"?`,{type:`template:forgotten-interpolation`,expression:e,stack:d()});return}let r=De.exec(t);if(!r)return;let i=r[1].replace(/\s*\.\s*/g,`.`);f(`Template contains "{${i}}". Did you mean "${"${"+i+`}`}"?`,{type:`template:forgotten-interpolation`,expression:i,stack:d()})}function je(e){return!!(e&&e[ke])}function Me(e,t){let n=e+t,r=n.lastIndexOf(`<`);if(r<=n.lastIndexOf(`>`))return!1;let i=n.slice(r);return/(?:^|\s)([^\s="'<>`]+)\s*=\s*(?:"[^"]*|'[^']*|[^\s"'<>`]*)$/.test(i)}function Ne(e){return e.reduce((t,n,r)=>{if(r===e.length-1)return t+n;let i=Me(t,n);if(!i&&/<\/?[A-Za-z0-9_-]*$/.test(n))throw Error(`Expressions cannot be used inside a tag name`);let a=i?`__MATRIX_ATTR_${r}__`:`<!--matrix:text:${r}-->`;return t+n+a},``)}function Pe(e){let t=[],n=e.ownerDocument.createTreeWalker(e,128),r=n.nextNode();for(;r;){let i=Ee.exec(r.data);i&&t.push({path:Ie(r,e),index:Number(i[1])}),r=n.nextNode()}return t}function Fe(e){let t=[],n=e.querySelectorAll(`*`);for(let r of n)for(let n of[...r.attributes]){let i=[...n.value.matchAll(Te)];if(i.length===0)continue;let a=[],o=0;for(let e of i)e.index>o&&a.push(n.value.slice(o,e.index)),a.push({index:Number(e[1])}),o=e.index+e[0].length;o<n.value.length&&a.push(n.value.slice(o)),t.push({path:Ie(r,e),name:n.name,parts:a})}return t}function Ie(e,t){let n=[],r=e;for(;r!==t;){let e=r.parentNode;if(!e)throw Error(`Matrix could not index a compiled template node`);n.unshift([...e.childNodes].indexOf(r)),r=e}return n}function Le(e,t){let n=e;for(let e of t)n=n.childNodes[e];return n}function Re(e,t){let n=we.get(e);n||(n=new WeakMap,we.set(e,n));let r=n.get(t);if(r)return r;let i=t.createElement(`template`);return i.innerHTML=Ne(e),r={template:i,textBindings:Pe(i.content),attributeBindings:Fe(i.content)},n.set(t,r),r}function ze(e,t){return e.map(e=>typeof e==`string`?e:t[e.index])}function Be(e){return!(!e||typeof e!=`object`||e.kind!==`signal`&&e.kind!==`computed`||typeof e.get!=`function`)}var Ve=new WeakMap,He=new WeakMap,Ue=Symbol(`matrix.style.result`),We=Symbol(`matrix.variables.result`),Ge=/[<>{};\u0000-\u001f\u007f]|(?:expression|behavior)\s*\(|(?:^|[^A-Za-z0-9_-])(?:javascript|vbscript|data):|url\(\s*["']?\s*(?:javascript|vbscript|data):/i;Object.freeze({"--matrix-color-primary":`#2563eb`,"--matrix-color-surface":`#ffffff`,"--matrix-color-text":`#0f172a`,"--matrix-space-1":`0.25rem`,"--matrix-space-2":`0.5rem`,"--matrix-space-3":`0.75rem`,"--matrix-radius-sm":`0.375rem`,"--matrix-radius-md":`0.5rem`,"--matrix-font-body":`system-ui, sans-serif`});function Ke(e){let t=2166136261;for(let n=0;n<e.length;n+=1)t^=e.charCodeAt(n),t=Math.imul(t,16777619);return Math.abs(t>>>0).toString(36)}function qe(e,t){return e.reduce((n,r,i)=>i===e.length-1?n+r:n+r+(t[i]??``),``)}function Je(e){return{[Ue]:!0,id:`matrix-global-${Ke(e)}`,scopeSelector:null,cssText:e}}function Ye(e){let t=Ve.get(e);return t||(t=new Map,Ve.set(e,t)),t}function Xe(e,t){let n=Ye(e);if(n.has(t.id))return n.get(t.id);let r=e.createElement(`style`);return r.setAttribute(`data-matrix-style`,t.id),r.textContent=t.cssText,e.head?.appendChild(r),r.parentNode||e.documentElement.appendChild(r),n.set(t.id,r),r}function Ze(e,...t){let n=typeof e==`string`?e:qe(e,t);if(typeof e!=`string`&&t.length===0){let t=He.get(e);if(t)return t;let r=Je(n);return He.set(e,r),r}return Je(n)}function Qe(e,t){let n=Be(t)?t.value:t;if(n==null||n===!1)return null;if(typeof n==`object`||typeof n==`function`||typeof n==`symbol`)throw TypeError(`CSS custom property "${e}" expects a primitive value or reactive value`);let r=String(n);if(Ge.test(r))throw Error(`Unsafe CSS custom property value rejected for ${e}`);return r}function $e(e){return!!(e&&e[Ue])}function et(e){return!!(e&&e[We])}function tt(e,t,n){if(!$e(t))throw TypeError(`use:style expects a css() result`);Xe(e.ownerDocument,t),t.scopeSelector&&e.setAttribute(`data-matrix-scope`,t.id),s(`style`,{type:`style:apply`,element:e,definition:t}),n.add(()=>{t.scopeSelector&&e.removeAttribute(`data-matrix-scope`)})}function nt(e,t,n){if(!et(t))throw TypeError(`use:vars expects a cssVariables() result`);let r=new Set;de(()=>{let n=new Set(Object.keys(t.values));for(let t of r)n.has(t)||e.style.removeProperty(t);for(let[n,r]of Object.entries(t.values)){let t=Qe(n,r);t===null?e.style.removeProperty(n):e.style.setProperty(n,t)}r.clear();for(let e of n)r.add(e)}),n.add(()=>{for(let t of r)e.style.removeProperty(t)})}function rt(e){return e.type===`checkbox`?e.checked:e.type===`radio`?e.checked?e.value:void 0:e.type===`file`?e.files:e.type===`number`||e.type===`range`?e.value===``?``:e.valueAsNumber:e.multiple&&e.options?[...e.selectedOptions].map(e=>e.value):e.value}function it(e,t){if(e.type===`checkbox`){e.checked=!!t;return}if(e.type===`radio`){e.checked=String(t??``)===e.value;return}if(e.type===`file`)return;if(e.multiple&&e.options&&Array.isArray(t)){let n=new Set(t.map(String));for(let t of e.options)t.selected=n.has(t.value);return}let n=t??``;e.value!==String(n)&&(e.value=n)}function at(e,t,n){let r=t,i=r?.source??r,a=Number(r?.debounce??0),o=r?.sanitize;if(o!==void 0&&typeof o!=`function`)throw TypeError(`use:bind sanitize expects a function`);if(!Be(i)||i.kind!==`signal`||typeof i.set!=`function`)throw TypeError(`use:bind expects a writable signal`);let s=!1,c,l=!1;de(()=>{let t=i.value;l||it(e,t)});let u=()=>{c=void 0;let t=rt(e);if(t!==void 0){l=!0;try{i.value=o?o(t):t}finally{l=!1}}},d=()=>{if(!s){if(a>0){clearTimeout(c),c=setTimeout(u,a);return}u()}},f=()=>{s=!0},p=()=>{s=!1,d()};e.addEventListener(`input`,d),e.addEventListener(`change`,d),e.addEventListener(`compositionstart`,f),e.addEventListener(`compositionend`,p),n.add(()=>{clearTimeout(c),e.removeEventListener(`input`,d),e.removeEventListener(`change`,d),e.removeEventListener(`compositionstart`,f),e.removeEventListener(`compositionend`,p)})}var ot=1,st=new Map;function ct(e){e.type?.startsWith(`dom:`)&&s(`renderer`,e),s(`logger`,e)}function lt(e){let t=`component-${ot++}`;return e.devtoolsId=t,st.set(t,e),t}function ut(e){e?.devtoolsId&&st.delete(e.devtoolsId)}var dt=Symbol(`matrix.keyed.list`);function ft(e){return!!(e&&e[dt])}var pt=`@`,mt=`.`,ht=`?`,gt=new Set([`href`,`src`,`action`,`formaction`,`poster`,`xlink:href`]),_t=new WeakSet;function vt(e){try{ct(e)}catch{}}function yt(e){return!(!e||typeof e!=`object`||e.kind!==`signal`&&e.kind!==`computed`||typeof e.get!=`function`)}function bt(e){return!!(e&&typeof e.nodeType==`number`&&typeof e.nodeName==`string`)}function xt(e){return e==null||typeof e==`boolean`||yt(e)||je(e)||ft(e)||Se(e)||Array.isArray(e)||bt(e)||typeof e==`function`}function St(e,t,n){if(xt(n))return;let r=l(n);if(e.invalidOutputWarnings.has(r))return;e.invalidOutputWarnings.add(r);let i=t.render.name||`anonymous`;u(`Component "${i}" returned ${r}. Return html\`...\`, a component, a Signal or Computed, an array, a DOM node, or null. The value will render as text.`,{type:`component:invalid-output`,name:i,valueType:typeof n,source:t.render})}function Ct(e,n,r){let i=n.length+r.length,{development:a,bindingWarningThreshold:o}=t();!a||i<=o||_t.has(e.strings)||(_t.add(e.strings),f(`Template has ${i} dynamic bindings. Split large views into components or move derived work into Computeds to keep updates local.`,{type:`performance:unoptimized-bindings`,bindingCount:i,textBindings:n.length,attributeBindings:r.length}))}function wt(e,t){let n=t.render.name;if(!n)return e;let r=`[${n}]`,i=e instanceof Error?e:Error(`${r} ${String(e)}`,{cause:e});i.message.startsWith(r)||(i.message=`${r} ${i.message}`);let a=t._matrixSourceLocation;if(i.stack){let e=i.stack.split(`
`);e[0]=`${i.name}: ${i.message}`,a&&!i.stack.includes(`Component "${n}" was created here`)&&(e.push(`\n[Matrix] Component "${n}" was created here:`),e.push(a)),i.stack=e.join(`
`)}return i}function Tt(e){return yt(e)?e.value:e}function Et(e){e.parentNode&&e.parentNode.removeChild(e)}function Dt(e,t){let n=e;for(;n;){let e=n.nextSibling;if(Et(n),n===t)break;n=e}}function Ot(e){let t;for(let n of[...e].reverse())try{n?.dispose?.()}catch(e){t??=e}if(t)throw t}function z(e){try{e?.dispose?.()}catch{}}function kt(e,t){if(!gt.has(e.toLowerCase())||typeof t!=`string`)return;let n=t.indexOf(`:`),r=n===-1?``:t.slice(0,n).replace(/[\u0000-\u0020\u007f]+/g,``).toLowerCase();if(r===`javascript`||r===`vbscript`||r===`data`)throw Error(`Unsafe dynamic URL rejected for attribute ${e}`)}function At(){return{firstNode:null,get nodes(){return[]},dispose(){},moveBefore(){}}}function jt(e,t,n){let r=t.ownerDocument.createTextNode(String(e));return t.insertBefore(r,n),{nodes:[r],firstNode:r,moveBefore(e){t.insertBefore(r,e)},dispose(){Et(r)}}}function Mt(e,t,n){return t.insertBefore(e,n),{nodes:[e],firstNode:e,moveBefore(n){t.insertBefore(e,n)},dispose(){Et(e)}}}function Nt(e,t,n,r){let i=[];try{for(let a of e)i.push(Ft(a,t,n,r))}catch(e){throw z({dispose:()=>Ot(i)}),e}let a=!1;return{get firstNode(){return i.find(e=>e.firstNode)?.firstNode??null},get nodes(){return i.flatMap(e=>e.nodes)},moveBefore(e){for(let t of i)t.moveBefore(e)},dispose(){a||(a=!0,Ot(i))}}}function Pt(e){return e===!0||e===!1?String(e):e}function Ft(e,t,n,r){let i=me(r),a=_e(),o=At(),s=!1,c=e=>a?L(a,()=>It(e,t,n,i)):It(e,t,n,i),l={get firstNode(){return o.firstNode},get nodes(){return o.nodes},moveBefore(e){o.moveBefore(e)},dispose(){if(s)return;s=!0;let e;try{o.dispose()}catch(t){e=t}try{i.dispose()}catch(t){e??=t}if(e)throw e}},u,d=!1;function f(e){if(d&&Object.is(u,e))return;if(typeof o.canUpdate==`function`&&o.canUpdate(e)){o.update(e),u=e,d=!0;return}let t=b(null,()=>c(e));try{o.dispose()}catch(e){throw z(t),e}o=t,u=e,d=!0}try{i.run(()=>{yt(e)?de(()=>{f(Pt(e.value)),ct({type:`dom:update`,kind:`content`,parent:t,source:e})},{name:`render-dynamic-value`,warnOnDependencyChange:!1}):f(Pt(e))})}catch(e){throw z(l),e}return l}function It(e,t,n,r){return e==null||e===!1||e===!0?At():yt(e)?Ft(e,t,n,r):je(e)?Wt(e,t,n,r):ft(e)?Gt(e,t,n,r):Se(e)?Lt(e,t,n,r):typeof e==`function`?Lt(xe(e),t,n,r):Array.isArray(e)?Nt(e,t,n,r):bt(e)?Mt(e,t,n):jt(e,t,n)}function Lt(e,t,n,r){let i=me(r),a=_e(),o={scope:i,mountCallbacks:[],parent:a,provides:new Map,isErrorBoundary:!!e[ge],result:e,stateScope:i,stateSlots:[],stateCursor:0,isRendering:!1,isMounted:!1,invalidOutputWarnings:new Set};lt(o);let s,c,l=me(i);try{l.run(()=>{s=Rt(o,e),c=L(o,()=>It(s,t,n,l))});let r=c.nodes.find(e=>e.nodeType===1)??c.nodes[0]??null;for(let e of o.mountCallbacks){let t=i.run(()=>e(r));typeof t==`function`&&i.add(t)}o.mountCallbacks.length=0,o.isMounted=!0,vt({type:`component:mount`,id:o.devtoolsId,name:e.render.name||`anonymous`,parentId:a?.devtoolsId??null})}catch(s){z(c),z(l),z(i),vt({type:`component:error`,id:o.devtoolsId,name:e.render.name||`anonymous`,message:s?.message??String(s)}),ut(o);let u=wt(s,e),d=a;for(;d;){if(d.isErrorBoundary&&!d.handling){d.handling=!0;try{return It(typeof d.result.fallback==`function`?d.result.fallback(u):d.result.fallback,t,n,r)}finally{d.handling=!1}}d=d.parent}throw u}return{get firstNode(){return c.firstNode},get nodes(){return c.nodes},moveBefore(e){c.moveBefore(e)},canUpdate(t){return e.update?.(t)===!0},update(r){if(!this.canUpdate(r))return!1;let a=s,u=c,d=l,f=e,p=o.stateSlots.slice();o.mountCallbacks.length=0;let m=me(i),h,g;try{m.run(()=>{h=Rt(o,r),g=L(o,()=>It(h,t,n,m))}),u.dispose(),d.dispose(),s=h,c=g,l=m,e=r,o.result=r,vt({type:`component:update`,id:o.devtoolsId,name:r.render.name||`anonymous`,parentId:o.parent?.devtoolsId??null})}catch(t){z(g),z(m);for(let e=p.length;e<o.stateSlots.length;e+=1)z(o.stateSlots[e]?.value);o.stateSlots.length=p.length;for(let e=0;e<p.length;e+=1)o.stateSlots[e]=p[e];throw o.mountCallbacks.length=0,s=a,c=u,l=d,e=f,o.result=f,wt(t,r)}return o.mountCallbacks.length=0,!0},dispose(){if(o.disposed)return;o.disposed=!0;let e;try{c.dispose()}catch(t){e=t}try{l.dispose()}catch(t){e??=t}try{i.dispose()}catch(t){e??=t}ut(o);try{vt({type:`component:unmount`,id:o.devtoolsId,name:o.result.render.name||`anonymous`})}catch(t){e??=t}if(e)throw e}}}function Rt(e,t){let n=e.stateSlots.length;e.stateCursor=0,e.isRendering=!0;try{let r=L(e,()=>T(e,()=>t.render(t.props)));if(e.isMounted&&e.stateCursor!==n)throw Error(`Component state order changed: expected ${n} slots, received ${e.stateCursor}`);return St(e,t,r),r}finally{e.isRendering=!1}}function zt(e,t){let n=ze(e,t).map(Tt);return n.length===1&&typeof e[0]!=`string`?n[0]:n.join(``)}function Bt(e,t){let n=ze(e,t);return n.length===1&&typeof e[0]!=`string`?n[0]:zt(e,t)}function Vt(e,t,n){if(t==null||t===!1)return e.removeAttribute(`style`),new Set;if(typeof t==`string`)return e.style.cssText=t,new Set;if(typeof t!=`object`)return e.style.cssText=String(t),new Set;let r=new Set(Object.keys(t));for(let t of n)r.has(t)||e.style.removeProperty(t);for(let[n,r]of Object.entries(t))r==null||r===!1?e.style.removeProperty(n):e.style.setProperty(n,r);return r}function Ht(e,t,n,r,i){let[a,...o]=t.split(`.`),s={once:o.includes(`once`),capture:o.includes(`capture`),passive:o.includes(`passive`)},c;de(()=>{let t=zt(n,r);c&&e.removeEventListener(a,c,s),typeof t==`function`?(c=e=>{o.includes(`prevent`)&&e.preventDefault(),o.includes(`stop`)&&e.stopPropagation(),t(e)},e.addEventListener(a,c,s)):c=void 0},{flush:`sync`}),i.add(()=>{c&&e.removeEventListener(a,c,s)})}function Ut(e,t,n,r){let{name:i,parts:a}=t,o=new Set;if(i===`use:style`){tt(e,Bt(a,n),r),e.removeAttribute(i);return}if(i===`use:vars`){nt(e,Bt(a,n),r),e.removeAttribute(i);return}if(i===`use:bind`){at(e,Bt(a,n),r),e.removeAttribute(i);return}if(i.startsWith(pt)){let t=i.slice(1);if(!t)throw Error(`Empty event name in Matrix template`);Ht(e,t,a,n,r);return}de(()=>{let t=zt(a,n);if(i.startsWith(mt)){let n=i.slice(1);if(!n)throw Error(`Empty property name in Matrix template`);kt(n,String(t??``)),e[n]!==t&&(e[n]=t);return}if(i.startsWith(ht)){let n=i.slice(1);if(!n)throw Error(`Empty boolean attribute name in Matrix template`);e.toggleAttribute(n,!!t);return}if(i===`style`){o=Vt(e,t,o);return}t==null||t===!1?e.removeAttribute(i):(kt(i,String(t)),e.setAttribute(i,String(t))),ct({type:`dom:update`,kind:`attribute`,element:e,name:i})}),r.add(()=>{i.startsWith(mt)?e[i.slice(1)]=void 0:e.removeAttribute(i)})}function Wt(e,t,n,r){let i=t.ownerDocument,a=Re(e.strings,i),o=me(r),s=i.createComment(`matrix:start`),c=i.createComment(`matrix:end`);t.insertBefore(s,n),t.insertBefore(c,n);let l=a.template.content.cloneNode(!0),u=a.textBindings.map(({path:e,index:t})=>({node:Le(l,e),index:t})),d=[],f=a.attributeBindings.map(({path:e,name:t,parts:n})=>({element:Le(l,e),name:t,parts:n}));Ct(e,u,f);try{o.run(()=>{for(let t of f)gt.has(t.name.toLowerCase())&&Ut(t.element,t,e.values,o)}),t.insertBefore(l,c),o.run(()=>{for(let t of u)d.push(Ft(e.values[t.index],t.node.parentNode,t.node,o));for(let t of f)gt.has(t.name.toLowerCase())||Ut(t.element,t,e.values,o)})}catch(e){throw z({dispose:()=>Ot(d)}),z(o),Dt(s,c),e}let p=!1;return{firstNode:s,get nodes(){let e=[],t=s.nextSibling;for(;t&&t!==c;)e.push(t),t=t.nextSibling;return e},moveBefore(e){let n=s;for(;n;){let r=n.nextSibling;if(t.insertBefore(n,e),n===c)break;n=r}},dispose(){if(p)return;p=!0;let e;try{Ot(d)}catch(t){e=t}try{o.dispose()}catch(t){e??=t}try{Dt(s,c)}catch(t){e??=t}if(e)throw e}}}function Gt(e,t,n,r){let i=me(r),a=_e(),o=t.ownerDocument.createComment(`matrix:keyed:start`),s=t.ownerDocument.createComment(`matrix:keyed:end`),c=new Map,l=[],d=!1,f=e=>a?L(a,e):e();t.insertBefore(o,n),t.insertBefore(s,n);function p(e){let n=o;for(;n;){let r=n.nextSibling;if(t.insertBefore(n,e),n===s)break;n=r}}function m(n){let r=Array.isArray(n)?n:[],a=new Map(c),o=[],d=new Map,f=[],p=[],m=new Set;for(let t of r){let n=e.getKey(t);if(m.has(n))throw u(`Duplicate list key "${String(n)}" detected before reconciliation. Every key in a keyed list must be unique.`,{type:`list:duplicate-key`,key:n}),Error(`Duplicate list key: ${String(n)}`);m.add(n),p.push(n)}try{for(let e=0;e<r.length;e+=1){let n=r[e],c=p[e],l=a.get(c);l?.canUpdate&&!l.canUpdate(n)&&(l.dispose(),l=void 0),l?.update&&(l.update(n)||(l.dispose(),l=void 0)),l||(l=It(n,t,s,i),f.push(l)),d.set(c,l),o.push(l)}let e=s;for(let t=o.length-1;t>=0;--t){let n=o[t];n.moveBefore(e),e=n.firstNode??e}for(let[e,t]of a)d.has(e)||t.dispose();c.clear();for(let[e,t]of d)c.set(e,t);l=o}catch(e){let t=new Set([...a.values(),...f]);throw c.clear(),l=[],z({dispose:()=>Ot(t)}),e}}try{i.run(()=>{yt(e.items)?de(()=>f(()=>m(e.items.value))):f(()=>m(e.items))})}catch(e){throw z({dispose(){Ot(l),i.dispose()}}),Dt(o,s),e}return{firstNode:o,get nodes(){let e=[],t=o.nextSibling;for(;t&&t!==s;)e.push(t),t=t.nextSibling;return e},moveBefore(e){p(e)},dispose(){if(d)return;d=!0;let e;try{i.dispose()}catch(t){e=t}try{Ot(l)}catch(t){e??=t}c.clear(),l=[];try{Dt(o,s)}catch(t){e??=t}if(e)throw e}}}function Kt(e,t,n={}){if(!t||typeof t.insertBefore!=`function`)throw TypeError(`mount() expects a DOM container`);let r=me(),i,a=!1,o=typeof e==`function`?xe(e,n):e;try{r.run(()=>{i=It(o,t,null,r)})}catch(e){throw z(r),e}return{get nodes(){return i.nodes},unmount(){if(a)return;a=!0;let e;try{i.dispose()}catch(t){e=t}try{r.dispose()}catch(t){e??=t}if(e)throw e}}}var qt=Symbol(`matrix.fragment`),Jt=new Map,Yt=new Map([[`className`,`class`],[`htmlFor`,`for`],[`readOnly`,`readonly`],[`autoFocus`,`autofocus`],[`autoComplete`,`autocomplete`],[`autoPlay`,`autoplay`],[`colSpan`,`colspan`],[`rowSpan`,`rowspan`],[`tabIndex`,`tabindex`]]),Xt=new Set([`checked`,`disabled`,`indeterminate`,`muted`,`selected`,`value`]),Zt=new Set([`area`,`base`,`br`,`col`,`embed`,`hr`,`img`,`input`,`link`,`meta`,`param`,`source`,`track`,`wbr`]);function B(e,t,n){return Qt(e,t,n)}function V(e,t,n){return Qt(e,t,n)}function Qt(e,t,n){let r=t??{},i=n??r.key;if(e===qt)return r.children??null;if(typeof e==`function`){let t={...r};return delete t.key,xe(e,t,i)}if(typeof e!=`string`||e.length===0)throw TypeError(`jsx() expects an element or Matrix component`);let a=$t(e,r);return i!==void 0&&Object.defineProperty(a,"key",{value:i,enumerable:!0}),a}function $t(e,t){let n=[],r=[];for(let[e,i]of Object.entries(t))if(e!==`children`&&e!==`key`){if(e===`dangerouslySetInnerHTML`)throw Error(`Matrix does not support dangerouslySetInnerHTML`);n.push(en(e)),r.push(i)}let i=Object.prototype.hasOwnProperty.call(t,`children`)?[t.children]:[];return R(tn(e,n,i.length),...r,...i)}function en(e){let t=/^on([A-Z].*)$/.exec(e);if(t){let e=t[1],n=[],r=!0;for(;r;){r=!1;for(let[t,i]of[[`Capture`,`capture`],[`Once`,`once`],[`Passive`,`passive`],[`Prevent`,`prevent`],[`Stop`,`stop`]])if(e.endsWith(t)){e=e.slice(0,-t.length),n.unshift(i),r=!0;break}}return`@${e.toLowerCase()}${n.map(e=>`.${e}`).join(``)}`}return Xt.has(e)?`.${e}`:Yt.get(e)??e}function tn(e,t,n){let r=`${e}\u0000${t.join(``)}\u0000${n}`,i=Jt.get(r);if(i)return i;let a=[`<${e}`];for(let e of t)a[a.length-1]+=` ${e}="`,a.push(`"`);a[a.length-1]+=`>`;for(let e=0;e<n;e+=1)a.push(``);return Zt.has(e.toLowerCase())||(a[a.length-1]+=`</${e}>`),Object.defineProperty(a,"raw",{value:a.slice()}),Jt.set(r,a),a}var nn=new Set([`signal`,`computed`]),H=e=>nn.has(e?.kind),U=(e,t)=>H(e)?e.value:e??t,rn=e=>e?.kind===`signal`;function an(e={}){let{class:t=``,size:n=`1em`,ariaLabel:r}=e;return{iconClass:t?`prism-icon ${t}`:`prism-icon`,size:n,ariaHidden:r===void 0?`true`:`false`,role:r===void 0?void 0:`img`,ariaLabel:r}}function on(e){let t=[`<svg class="`,`" width="`,`" height="`,`" viewBox="0 0 24 24" fill="none" aria-hidden="`,`" role="`,`" aria-label="`,`" focusable="false">${e}</svg>`];return Object.defineProperty(t,"raw",{value:t.slice()}),t}function W(e){let t=on(e);return(e={})=>{let{iconClass:n,size:r,ariaHidden:i,role:a,ariaLabel:o}=an(e);return R(t,n,r,r,i,a,o)}}W(`<circle cx="12" cy="12" r="5.5" fill="currentColor" />`),W(`<path d="M3.5 12s3-5 8.5-5 8.5 5 8.5 5-3 5-8.5 5-8.5-5-8.5-5Z" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round" /><circle cx="12" cy="12" r="2.25" fill="currentColor" />`),W(`<path class="prism-tree-toggle-bar prism-tree-toggle-bar-horizontal" d="M7 12h10" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" /><path class="prism-tree-toggle-bar prism-tree-toggle-bar-vertical" d="M12 7v10" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" />`),W(`<circle cx="12" cy="12" r="3.25" fill="currentColor" />`),W(`<circle cx="12" cy="12" r="2.75" fill="currentColor" />`),W(`<circle cx="12" cy="12" r="4" fill="currentColor" />`),W(`<circle cx="12" cy="12" r="3" fill="currentColor" />`),W(`<g stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><path d="M12 5v14" /><path d="M5 12h14" /></g>`),W(`<path d="M5 12h14" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" />`);var sn=W(`<g stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><path d="m7 7 10 10" /><path d="m17 7-10 10" /></g>`);W(`<g stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="10.8" cy="10.8" r="5.8" /><path d="m15.2 15.2 4.3 4.3" /></g>`),W(`<path d="M4.5 6h15l-5.8 6.6v4.5L10.3 19v-6.4L4.5 6Z" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round" />`),W(`<g fill="currentColor"><circle cx="6" cy="12" r="1.7" /><circle cx="12" cy="12" r="1.7" /><circle cx="18" cy="12" r="1.7" /></g>`),W(`<path d="M12 19V5m0 0-5 5m5-5 5 5" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" />`),W(`<path d="M12 5v14m0 0-5-5m5 5 5-5" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" />`),W(`<path d="M19 12H5m0 0 5-5m-5 5 5 5" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" />`),W(`<path d="M5 12h14m0 0-5-5m5 5-5 5" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" />`),W(`<path d="m6.5 9.5 5.5 5 5.5-5" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" />`),W(`<path d="m9.5 6.5 5 5.5-5 5.5" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" />`),W(`<g stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"><rect x="4" y="6.5" width="16" height="11" rx="2" /><path d="m5 8 7 5 7-5" /></g>`),W(`<path d="M6 5.5h12a3 3 0 0 1 3 3v5a3 3 0 0 1-3 3h-5.2L8 19.5v-3H6a3 3 0 0 1-3-3v-5a3 3 0 0 1 3-3Z" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round" />`),W(`<g stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="M18 10a6 6 0 0 0-12 0c0 5-2 5-2 6h16c0-1-2-1-2-6Z" /><path d="M10 20h4" /></g>`);var cn=W(`<g fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="m10 14-1.8 1.8a3.3 3.3 0 0 1-4.7-4.7L6 8.6a3.3 3.3 0 0 1 4.7 0" /><path d="m14 10 1.8-1.8a3.3 3.3 0 0 1 4.7 4.7L18 15.4a3.3 3.3 0 0 1-4.7 0" /><path d="m8.5 12h7" /></g>`);W(`<g fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><circle cx="18" cy="5.5" r="2.4" /><circle cx="6" cy="12" r="2.4" /><circle cx="18" cy="18.5" r="2.4" /><path d="m8.2 10.8 7.6-4.1M8.2 13.2l7.6 4.1" /></g>`);var ln=W(`<path d="m4 5 16 7-16 7 3.2-6.1L13 12 7.2 11.1 4 5Z" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round" />`),un=W(`<path d="m5 12.5 4.5 4.5L19 7" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" />`);W(`<g fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"><path d="m12 4 9 15H3L12 4Z" /><path d="M12 9v4" stroke-linecap="round" /><circle cx="12" cy="16.5" r=".8" fill="currentColor" stroke="none" /></g>`),W(`<g fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round"><circle cx="12" cy="12" r="8.5" /><path d="M12 11v5" /><path d="M12 8h.01" /></g>`),W(`<g fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round"><circle cx="12" cy="12" r="8.5" /><path d="M9.8 9.3a2.4 2.4 0 1 1 3.7 2c-1 .7-1.5 1.1-1.5 2.2" /><path d="M12 16.5h.01" /></g>`),W(`<path d="M20 12a8 8 0 1 1-2.3-5.7" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" />`);var dn=W(`<g fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"><rect x="5" y="10" width="14" height="10" rx="2" /><path d="M8 10V7.8a4 4 0 0 1 8 0V10" /><circle cx="12" cy="15" r="1" fill="currentColor" stroke="none" /></g>`);W(`<g fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"><rect x="5" y="10" width="14" height="10" rx="2" /><path d="M8 10V7.8a4 4 0 0 1 7.1-2.5" /><circle cx="12" cy="15" r="1" fill="currentColor" stroke="none" /></g>`),W(`<g fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"><path d="M7 3.5h7l4 4v13H7v-17Z" /><path d="M14 3.5v4h4" /></g>`),W(`<path d="M3.5 7.5a2 2 0 0 1 2-2h4l2 2h7a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2v-9Z" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round" />`),W(`<g fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"><rect x="4" y="5" width="16" height="14" rx="2" /><circle cx="9" cy="9.5" r="1.4" /><path d="m5 17 4.5-4 3 2.5 2.2-2 4.3 3.5" /></g>`),W(`<g fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M12 4v11m0 0-4-4m4 4 4-4" /><path d="M5 19h14" /></g>`),W(`<g fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M12 20V9m0 0-4 4m4-4 4 4" /><path d="M5 5h14" /></g>`);var fn=W(`<g fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"><rect x="8" y="8" width="11" height="12" rx="2" /><path d="M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h2" /></g>`);W(`<g fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"><rect x="4" y="5.5" width="16" height="15" rx="2" /><path d="M8 3.5v4M16 3.5v4M4 10h16" /><path d="M8 14h.01M12 14h.01M16 14h.01M8 17h.01M12 17h.01" stroke-linecap="round" /></g>`);var pn=W(`<g fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="8.5" /><path d="M12 7v5l3.5 2" /></g>`);W(`<path d="M19 10.2c0 4.7-7 10.3-7 10.3S5 14.9 5 10.2a7 7 0 1 1 14 0Z" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round" /><circle cx="12" cy="10" r="2.2" fill="currentColor" />`),W(`<g fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="8" r="3.2" /><path d="M5 20a7 7 0 0 1 14 0" /></g>`),W(`<g fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><circle cx="8.2" cy="8" r="3" /><path d="M3.5 19a4.7 4.7 0 0 1 9.4 0" /><path d="M17 12.5v7M13.5 16h7" /></g>`),W(`<g fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><circle cx="9" cy="8" r="2.8" /><circle cx="16.5" cy="9" r="2.2" /><path d="M3.8 19a5.3 5.3 0 0 1 10.5 0M15 14.7a4.4 4.4 0 0 1 5.2 4.3" /></g>`),W(`<path d="m12 3 1.2 2.3 2.5.6 2-1.2 1.7 1.7-1.2 2 .6 2.5L21 12l-2.2 1.1-.6 2.5 1.2 2-1.7 1.7-2-1.2-2.5.6L12 21l-1.1-2.3-2.5-.6-2 1.2-1.7-1.7 1.2-2-.6-2.5L3 12l2.3-1.1.6-2.5-1.2-2 1.7-1.7 2 1.2 2.5-.6L12 3Z" fill="none" stroke="currentColor" stroke-width="1.35" stroke-linejoin="round" /><circle cx="12" cy="12" r="2.5" fill="none" stroke="currentColor" stroke-width="1.7" />`),W(`<path d="m12 3 1.6 6.4L20 11l-6.4 1.6L12 19l-1.6-6.4L4 11l6.4-1.6L12 3Z" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round" /><path d="m19 16 .6 2.4L22 19l-2.4.6L19 22l-.6-2.4L16 19l2.4-.6L19 16Z" fill="currentColor" />`),W(`<g fill="none" stroke="currentColor" stroke-width="1.7"><rect x="4" y="4" width="6" height="6" rx="1" /><rect x="14" y="4" width="6" height="6" rx="1" /><rect x="4" y="14" width="6" height="6" rx="1" /><rect x="14" y="14" width="6" height="6" rx="1" /></g>`),W(`<g fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><path d="M8 6h11M8 12h11M8 18h11" /><path d="M4.5 6h.01M4.5 12h.01M4.5 18h.01" /></g>`);var mn=W(`<g fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="m9 8-4 4 4 4M15 8l4 4-4 4M13.5 5l-3 14" /></g>`);W(`<g fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><rect x="4" y="5" width="16" height="14" rx="2" /><path d="m8 10 2 2-2 2M13 14h3" /></g>`),W(`<g fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="M4 5 20 19" /><path d="M10.5 7.2A9.5 9.5 0 0 1 12 7c5.5 0 8.5 5 8.5 5a15 15 0 0 1-3.1 3.4M6.6 9.1C4.7 10.4 3.5 12 3.5 12s3 5 8.5 5c.7 0 1.4-.1 2-.2" /></g>`);var hn=0,gn=new Set([`success`,`info`,`warning`,`error`]);function _n(e={}){let{ariaLabel:t,children:n,class:r=``,dismissible:i=!1,id:a,onDismiss:o,role:s,title:c,tone:l=`info`}=e,u=F(()=>{let e=U(l,`info`);return gn.has(e)?e:`info`}),d=F(()=>U(c)),f=n??e.description,p=`${a??`prism-alert-${++hn}`}-title`,m=`${a??`prism-alert-${hn}`}-description`,h=s??(u.value===`error`||u.value===`warning`?`alert`:`status`),g=d.value!==void 0&&d.value!==null&&d.value!==``,_=f!=null&&f!==``;return R`
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
        ${g?R`<div class="prism-alert-title" id="${p}">${d.value}</div>`:``}
        ${_?R`<div class="prism-alert-description" id="${m}">${f}</div>`:``}
      </div>
      ${i?R`
        <button class="prism-alert-dismiss" type="button" aria-label="Dismiss" @click=${o}>
          ${sn({size:16})}
        </button>
      `:``}
    </div>
  `}var vn=(e,t=!1)=>!!U(e,t),yn=(e,t,n)=>{let r=U(e,n);return t.has(r)?r:n},bn=new Set([`top`,`top-start`,`top-end`,`bottom`,`bottom-start`,`bottom-end`,`left`,`right`]);function xn(e,t=`bottom`){return bn.has(e)?e:t}var Sn=`prism-badge`;function Cn(e={}){let{value:t,children:n=[],tone:r=`neutral`,size:i=`medium`,pulseOnChange:a=!1,class:o=``,ariaLabel:s}=e,c=t===void 0?n:t,l=H(r)?r:r||`neutral`,u=H(i)?i:i||`medium`,d=e=>R`<span class="${Sn} ${Sn}-${l} ${Sn}-${u} ${e?`${Sn}-pulse`:``} ${o}" role="${s?`img`:void 0}" aria-label="${s}">${c}</span>`;if(!a||!H(c))return d(!1);let f,p=!1;return F(()=>{let e=c.value,t=p&&!Object.is(e,f);return f=e,p=!0,d(t)})}var G=`prism-button`,wn=new Set([`primary`,`secondary`,`tertiary`,`error`,`warning`,`information`,`success`]),Tn=new Set([`small`,`medium`,`large`]),En=new Set([`rounded`,`pill`,`square`]),Dn=new Set([`start`,`end`]),On=new Set([`cobalt`,`iris`,`teal`]),K=U;function kn(e={}){let{children:t=[],label:n,showLabel:r=!0,icon:i,iconPosition:a=`start`,class:o=``,id:s,type:c=`button`,name:l,value:u,variant:d=`primary`,size:f=`medium`,shape:p=`rounded`,palette:m,fullWidth:h=!1,loading:g=!1,loadingLabel:_=`Loading`,pressed:v,disabled:y=!1,ariaLabel:b,title:x,onClick:S,onFocus:C,onBlur:w}=e,T=F(()=>{let e=yn(d,wn,`primary`),t=yn(f,Tn,`medium`),n=yn(p,En,`rounded`),i=vn(r,!0);return[G,`${G}-${e}`,`${G}-${t}`,`${G}-${n}`,i?``:`${G}-icon-only`,K(h,!1)?`${G}-full-width`:``,K(g,!1)?`${G}-loading`:``,K(v,!1)?`${G}-pressed`:``,o].filter(Boolean).join(` `)}),E=F(()=>{let e=K(r,!0),o=Dn.has(K(a))?K(a):`start`,s=K(g,!1)?K(_,`Loading`):n===void 0?t:n,c=K(g,!1)?R`<span class="${G}-spinner" aria-hidden="true"></span>`:K(i),l=c==null?null:R`<span class="${G}-icon" aria-hidden="true">${c}</span>`,u=e?R`<span class="${G}-label">${s}</span>`:null;return o===`end`&&e?R`${u}${l}`:R`${l}${u}`}),D=F(()=>K(y,!1)||K(g,!1)),O=F(()=>String(K(g,!1))),ee=F(()=>v===void 0?void 0:String(K(v,!1)));return R`<button type="${c}" class="${T}" id="${s}" name="${l}" value="${u}" title="${x}" data-prism-palette="${F(()=>{let e=K(m);return On.has(e)?e:void 0})}" aria-label="${F(()=>{let e=K(b);if(e!==void 0)return e;if(K(g,!1))return K(_,`Loading`);if(!K(r,!0)){let e=K(n===void 0?t:n);return typeof e==`string`||typeof e==`number`?String(e):`Button`}let i=K(n===void 0?t:n);return typeof i==`string`||typeof i==`number`?void 0:`Button`})}" aria-busy="${O}" aria-pressed="${ee}" ?disabled=${D} @click=${e=>{let t=K(S);typeof t==`function`&&t(e)}} @focus=${C} @blur=${w}>${E}</button>`}async function An(e){let t=String(e??``);if(globalThis.navigator?.clipboard?.writeText){await globalThis.navigator.clipboard.writeText(t);return}if(typeof document>`u`)throw Error(`Clipboard unavailable`);let n=document.createElement(`textarea`);n.value=t,n.setAttribute(`readonly`,``),n.setAttribute(`aria-hidden`,`true`),n.style.position=`fixed`,n.style.top=`0`,n.style.left=`-9999px`,n.style.opacity=`0`,(document.body??document.documentElement).append(n);try{if(n.select(),!document.execCommand?.(`copy`))throw Error(`Clipboard unavailable`)}finally{n.remove()}}var jn=`prism-code-viewer`,q=`prism-code`,Mn=new Set([`javascript`,`jsx`,`typescript`,`tsx`,`json`,`css`,`html`,`xml`,`bash`,`text`]),Nn=new Map([[`js`,`javascript`],[`mjs`,`javascript`],[`cjs`,`javascript`],[`ts`,`typescript`],[`mts`,`typescript`],[`cts`,`typescript`],[`sh`,`bash`],[`shell`,`bash`],[`zsh`,`bash`],[`plaintext`,`text`],[`plain`,`text`],[`txt`,`text`]]),Pn=new Set(`as.async.await.break.case.catch.class.const.continue.debugger.default.delete.do.else.export.extends.finally.for.from.function.get.if.import.in.instanceof.let.new.of.return.set.static.super.switch.throw.try.typeof.var.void.while.with.yield`.split(`.`)),Fn=new Set([`boolean`,`interface`,`keyof`,`never`,`number`,`string`,`type`,`unknown`,`void`]),In=new Set([`false`,`null`,`true`,`undefined`]),Ln=new Set([`html`,`xml`,`jsx`,`tsx`]),Rn=0,J=U;function zn(e){let t=String(e??`javascript`).toLocaleLowerCase(),n=Nn.get(t)??t;return Mn.has(n)?n:`text`}function Y(e,t,n){if(!n)return;let r=e[e.length-1];if(r?.type===t){r.value+=n;return}e.push({type:t,value:n})}function Bn(e,t,n){let r=t+1;for(;r<e.length;){if(e[r]===`\\`){r+=2;continue}if(e[r]===n)return r+1;r+=1}return e.length}function Vn(e,t){let n=e.slice(t).match(/^[A-Za-z_$][\w$-]*/);return n?n[0]:``}function Hn(e,t){let n=e.slice(t).match(/^(?:0[xob][\da-f]+|(?:\d+\.?\d*|\.\d+)(?:e[+-]?\d+)?n?)/i);return n?n[0]:``}function Un(e,t){let n=e.slice(t).match(/^(?:===|!==|=>|==|!=|<=|>=|&&|\|\||\?\?|\+\+|--|\+=|-=|\*=|\/=|\.\.\.|[=+\-*%!?<>:&|^~])/);return n?n[0]:``}function Wn(e,t){let n=[],r=Ln.has(t),i=t===`css`,a=[`javascript`,`jsx`,`typescript`,`tsx`,`json`].includes(t),o=0,s=!1,c=!1;for(;o<e.length;){if((a||i)&&e.startsWith(`//`,o)){let t=e.indexOf(`
`,o),r=t===-1?e.length:t;Y(n,`comment`,e.slice(o,r)),o=r;continue}if(e.startsWith(`/*`,o)){let t=e.indexOf(`*/`,o+2),r=t===-1?e.length:t+2;Y(n,`comment`,e.slice(o,r)),o=r;continue}let l=e[o];if(r&&l===`<`&&/[\w/>]/.test(e[o+1]??``)){let t=e[o+1]===`/`;Y(n,`tag`,t?`</`:`<`),o+=t?2:1,s=!0,c=!0;continue}if(r&&l===`>`&&s){Y(n,`punctuation`,l),o+=1,s=!1,c=!1;continue}if(l==="`"||l===`"`||l===`'`){let t=Bn(e,o,l);Y(n,`string`,e.slice(o,t)),o=t;continue}let u=Vn(e,o);if(u){let r=e.slice(o+u.length),a=r.match(/^\s*(.)/)?.[1],l=`plain`;c?(l=`tag-name`,c=!1):s&&/^\s*=/.test(r)?l=`attribute`:In.has(u)?l=`boolean`:Pn.has(u)||(t===`typescript`||t===`tsx`)&&Fn.has(u)?l=`keyword`:i&&/^\s*:/.test(r)||e.slice(0,o).match(/\.\s*$/)?l=`property`:a===`(`?l=`function`:t===`json`&&/^\s*:/.test(r)&&(l=`property`),Y(n,l,u),o+=u.length;continue}let d=Hn(e,o);if(d){Y(n,`number`,d),o+=d.length;continue}let f=Un(e,o);if(f){Y(n,`operator`,f),o+=f.length;continue}`{}[]();,.:`.includes(l)?Y(n,`punctuation`,l):Y(n,`plain`,l),o+=1}return n}function Gn(e,t){return Wn(String(e??``),t).map(e=>e.type===`plain`?e.value:R`<span class="${q}-token ${q}-token-${e.type}">${e.value}</span>`)}function Kn(e){return F(()=>{let t=J(e.syntaxColors,{}),n=J(e.style),r=typeof n==`object`&&n?{...n}:n,i={"--prism-code-font-family":J(e.fontFamily),"--prism-code-font-size":J(e.fontSize),"--prism-code-line-height":J(e.lineHeight),"--prism-code-tab-size":J(e.tabSize),"--prism-code-min-height":J(e.minHeight),"--prism-code-max-height":J(e.maxHeight)};for(let[e,n]of Object.entries(t??{}))i[`--prism-code-${e}`]=n;return typeof r==`string`?[r,Object.entries(i).filter(([,e])=>e!=null).map(([e,t])=>`${e}: ${t}`).join(`; `)].filter(Boolean).join(`; `):Object.fromEntries(Object.entries({...r??{},...i}).filter(([,e])=>e!=null))})}function qn(e,t=``){return H(e)?e:k(String(J(e,t)))}function Jn(e){if(e.label)return e.label;let t=zn(e.language);return t===`javascript`?`JavaScript`:t===`jsx`?`JSX`:t}function Yn(e){let t=J(e.tabs);return Array.isArray(t)&&t.length>0?t.map((t,n)=>({id:String(t.id??t.language??n),label:Jn(t),language:t.language??`javascript`,filename:t.filename??e.filename??`untitled.js`,code:qn(t.code,``)})):[{id:`source`,label:Jn({language:e.language}),language:e.language??`javascript`,filename:e.filename??`untitled.js`,code:qn(e.code,``)}]}function Xn(e={}){let{lineNumbers:t=!0,editable:n=!0,copyable:r=!0,syntaxColors:i={},fontFamily:a,fontSize:o,lineHeight:s,tabSize:c,minHeight:l,maxHeight:u,style:d,class:f=``,id:p,ariaLabel:m=`Code viewer`,onChange:h,onCopy:g,onTabChange:_}=e,v=Yn(e),y=v.length>1,b=p??`${jn}-${Rn+=1}`,x=`${b}-panel`,S=e=>`${b}-tab-${e}`,C=v.some(e=>e.id===`jsx`)?`jsx`:v[0].id,w=H(e.activeTab)?e.activeTab:k(String(J(e.activeTab,J(e.defaultTab,C)))),T=F(()=>v.find(e=>e.id===w.value)??v[0]),E=F(()=>T.value.code.value),D=F(()=>zn(J(T.value.language,`javascript`))),O=F(()=>J(T.value.filename,`untitled.js`)),ee=F(()=>Gn(E.value,D.value)),te=F(()=>Array.from({length:String(E.value??``).split(`
`).length},(e,t)=>R`<span class="${q}-gutter-line">${t+1}</span>`)),ne=F(()=>J(t,!0)?`${q}-gutter`:`${q}-gutter ${q}-gutter-hidden`),re=Kn({syntaxColors:i,fontFamily:a,fontSize:o,lineHeight:s,tabSize:c,minHeight:l,maxHeight:u,style:d}),A=k(`ready`),j=F(()=>String(J(m,`Code viewer`)??``).trim()||`Code viewer`),M=e=>{rn(w)&&(w.value=e),_?.(e)},N=(e,t)=>{if(!y)return;let n=t.key===`Home`?0:t.key===`End`?v.length-1:(e+(t.key===`ArrowLeft`?-1:1)+v.length)%v.length,r=v[n];r&&(t.preventDefault(),M(r.id),t.currentTarget.parentElement?.children[n]?.focus())},ie=F(()=>Math.max(0,v.findIndex(e=>e.id===T.value.id))),P=F(()=>A.value===`copied`?`Code copied to clipboard`:A.value===`error`?`Copy failed`:``),ae=e=>{let t=v.find(e=>e.id===w.value)??v[0];rn(t.code)&&(t.code.value=e.currentTarget.value,h?.(e))},oe=e=>{let t=e.currentTarget,n=t.parentElement,r=n?.querySelector(`.${q}-highlight`),i=n?.parentElement?.querySelector(`.${q}-gutter-lines`);r&&(r.style.transform=`translate(${-t.scrollLeft}px, ${-t.scrollTop}px)`),i&&(i.style.transform=`translateY(${-t.scrollTop}px)`)},se=async e=>{let t=String(E.value??``);try{await An(t),A.value=`copied`,g?.(t,e)}catch{A.value=`error`}setTimeout(()=>{A.value=`ready`},1400)},ce=F(()=>A.value===`copied`?`Code copied`:A.value===`error`?`Copy failed`:`Copy code`);return R`
    <section class="${jn} ${jn}-language-${D} ${y?`${jn}-has-tabs`:``} ${f}" id="${p}" style="${re}" aria-label="${j}" data-copy-state="${A}">
      <header class="${q}-header">
        <div class="${q}-file">
          <span class="${q}-file-dot" aria-hidden="true"></span>
          <span class="${q}-filename">${O}</span>
          ${y?null:R`<span class="${q}-language">${D}</span>`}
        </div>
        ${y?R`
          <div class="${q}-tabs" role="tablist" aria-label="Source language">
            ${v.map((e,t)=>R`
              <button
                type="button"
                role="tab"
                class="${q}-tab"
                id="${S(t)}"
                aria-controls="${x}"
                aria-selected="${F(()=>T.value.id===e.id?`true`:`false`)}"
                tabindex="${F(()=>T.value.id===e.id?`0`:`-1`)}"
                @click=${()=>M(e.id)}
                @keydown=${e=>{[`ArrowLeft`,`ArrowRight`,`Home`,`End`].includes(e.key)&&N(t,e)}}
              >${e.label}</button>
            `)}
          </div>
        `:null}
        <button type="button" class="${q}-copy" aria-label="${ce}" title="${ce}" ?hidden=${F(()=>!J(r,!0))} @click=${se}>${fn({size:`1em`})}</button>
        <span class="${q}-status" role="status" aria-live="polite" aria-atomic="true">${P}</span>
      </header>
      <div class="${q}-body" role="${y?`tabpanel`:void 0}" id="${y?x:void 0}" aria-labelledby="${y?F(()=>S(ie.value)):void 0}" tabindex="${y?0:void 0}">
        <div class="${ne}" aria-hidden="true"><span class="${q}-gutter-lines">${te}</span></div>
        <div class="${q}-scroll">
          <pre class="${q}-highlight" aria-hidden="true"><code>${ee}</code></pre>
          <textarea class="${q}-input" spellcheck="false" wrap="off" aria-label="${F(()=>`${j.value} source`)}" .value=${E} ?readonly=${F(()=>!J(n,!0)||!rn(T.value.code))} @input=${ae} @scroll=${oe}></textarea>
        </div>
      </div>
    </section>
  `}function Zn(e={}){let{action:t,children:n,class:r=``,description:i,icon:a,onRetry:o,retryLabel:s=`Try again`,status:c=`empty`,title:l=`Nothing here yet`}=e,u=F(()=>U(c,`empty`)),d=F(()=>U(l,`Nothing here yet`)),f=F(()=>U(i??n)),p=F(()=>typeof t==`function`?t():t),m=o?R`<button class="prism-button prism-button-secondary" type="button" @click=${o}>${s}</button>`:``;return R`
    <section class="prism-empty-state prism-empty-state-${u.value} ${r}" role="status">
      ${a?R`<div class="prism-empty-state-icon" aria-hidden="true">${a}</div>`:``}
      <h3>${d.value}</h3>
      ${f.value?R`<p>${f.value}</p>`:``}
      ${p.value||m?R`<div class="prism-empty-state-actions">${p.value}${m}</div>`:``}
    </section>
  `}var Qn=0,$n=e=>e!=null&&e!==``;function er(e={}){let{children:t,class:n=``,control:r,error:i,errorClass:a=``,hint:o,hintClass:s=``,id:c,label:l,labelClass:u=``,required:d=!1,style:f}=e,p=k();c===void 0&&p.peek()===void 0&&(p.value=`prism-form-field-${++Qn}`);let m=c??p.peek(),h=`${m}-hint`,g=`${m}-error`,_=F(()=>!!U(d,!1)),v=F(()=>U(o)),y=F(()=>U(i)),b=F(()=>$n(y.value)),x=F(()=>{let e=[];return $n(v.value)&&e.push(h),b.value&&e.push(g),e.length?e.join(` `):void 0}),S=typeof r==`function`?r({id:m,ariaDescribedBy:x,ariaInvalid:b,required:_}):t,C=F(()=>_.value?R`<span class="prism-form-field-required" aria-hidden="true">*</span>`:null),w=F(()=>$n(v.value)?R`<div class="prism-form-field-hint ${s}" id="${h}">${v.value}</div>`:null),T=F(()=>b.value?R`<div class="prism-form-field-error ${a}" id="${g}" role="alert">${y.value}</div>`:null);return R`
    <div class="prism-form-field ${n}" style="${f??``}">
      ${$n(l)?R`
        <label class="prism-form-field-label ${u}" for="${m}">
          <span>${l}</span>
          ${C}
        </label>
      `:``}
      <div class="prism-form-field-control">
        ${S}
      </div>
      ${w}
      ${T}
    </div>
  `}var X=`prism-select`,tr=new Set([`bottom`,`top`,`left`,`right`]),nr=new Set([`small`,`medium`,`large`]),rr=0;function ir(e){return xn(yn(e,tr,`bottom`),`bottom`)}function ar(e){return e&&typeof e==`object`?{value:e.value??e.label??``,label:e.label??e.value??``,disabled:e.disabled??!1}:{value:e??``,label:e??``,disabled:!1}}function or({triggerId:e,required:t,disabled:n,formValue:r,invalid:i}){return Ce(()=>{if(typeof document>`u`)return;let a=document.getElementById(e),o=a?.closest(`form`);if(!o)return;let s=()=>!!U(t),c=()=>!!U(n),l=e=>{if(!s()||c()||r.value!==``){i.value=!1;return}e.preventDefault(),i.value=!0,a.focus()};return o.addEventListener(`submit`,l,!0),()=>o.removeEventListener(`submit`,l,!0)}),null}function sr(e={}){let{options:t=[],value:n=``,onChange:r,onRender:i,id:a,name:o,placeholder:s=`Select an option`,disabled:c=!1,required:l=!1,size:u=`medium`,placement:d=`bottom`,ariaLabel:f,ariaDescription:p,ariaDescribedBy:m,ariaInvalid:h,error:g,class:_=``,style:v,onFocus:y,onBlur:b}=e,x=k(!1),S=k(!1),C=rn(n)?n:k(U(n)),w=k(`bottom`),T=k(-1),E=F(()=>yn(u,nr,`medium`)),D=H(t)?F(()=>(t.value??[]).map(ar)):(t??[]).map(ar),O=()=>H(D)?D.value:D,ee=(e,t)=>typeof i==`function`?i(e,t):e.label,te=F(()=>{let e=String(C.value??``);return O().find(t=>String(t.value)===e)}),ne=F(()=>{let e=te.value;return e?ee(e,{location:`trigger`,selected:!0}):s}),re=F(()=>String(C.value??``)),A=()=>!!U(l),j=F(()=>U(g)),M=F(()=>j.value!==void 0&&j.value!==null&&j.value!==``),N=F(()=>M.value?j.value:U(p)),ie=F(()=>N.value!==void 0&&N.value!==null&&N.value!==``),P,ae=()=>{},oe=a??`prism-select-${rr+=1}`,se=a??`${oe}-trigger`,ce=`${se}-listbox`,le=e=>`${se}-option-${e}`,ue=`${se}-message`,de=F(()=>[U(m),ie.value?ue:void 0].filter(Boolean).join(` `)||void 0),fe=F(()=>String(U(f,`Select an option`)??``).trim()||`Select an option`),pe=F(()=>{let e=U(h);return S.value||M.value||!!e}),me=F(()=>U(v)),I=()=>{let e=String(C.value??``);return O().findIndex(t=>String(t.value)===e)},he=(e,t)=>{let n=O();if(n.length===0)return-1;for(let r=1;r<=n.length;r+=1){let i=(e+t*r+n.length*2)%n.length;if(!n[i].disabled)return i}return-1},ge=()=>{let e=I();return e>=0&&!O()[e].disabled?e:he(-1,1)},L=e=>{w.value=ir(U(d)),T.value=e>=0?e:ge(),x.value=!0,be(),ve()},_e=()=>{if(!P||!x.value)return;let e=P.parentElement,t=e?.querySelector(`.${X}-menu`);if(!e||!t)return;t.style.position=``,t.style.top=``,t.style.right=``,t.style.bottom=``,t.style.left=``,t.style.width=``,t.style.maxHeight=``,t.style.zIndex=``,t.style.visibility=``;let n=t.hidden;t.hidden=!1;let r=Math.max(t.scrollHeight,t.offsetHeight),i=Math.max(t.scrollWidth,t.offsetWidth);t.hidden=n;let a=P.getBoundingClientRect(),o=Math.max(8,window.innerWidth-16),s=e.getBoundingClientRect(),c={bottom:window.innerHeight-a.bottom-8,top:a.top-8,right:window.innerWidth-a.right-8,left:a.left-8},l={bottom:r<=c.bottom,top:r<=c.top,right:i<=c.right,left:i<=c.left},u=ir(U(d)),f={bottom:`top`,top:`bottom`,right:`left`,left:`right`}[u],p=l[u]?u:l[f]||c[f]>c[u]?f:u;t.style.maxWidth=`${o}px`,p===`bottom`||p===`top`?(t.style.left=`${Math.max(8-s.left,0)}px`,t.style.right=`${Math.max(s.right-(window.innerWidth-8),0)}px`):p===`right`&&a.right+i+8>window.innerWidth?(t.style.left=`auto`,t.style.right=`0`):p===`left`&&a.left-i-8<0&&(t.style.right=`auto`,t.style.left=`0`);let m=p===`top`?c.top:p===`bottom`?c.bottom:Math.max(c.top,c.bottom);t.style.maxHeight=`${Math.min(288,Math.max(8,m))}px`,w.value=p},ve=()=>{if(typeof requestAnimationFrame==`function`){requestAnimationFrame(_e);return}setTimeout(_e,0)},ye=e=>{let t=P?.parentElement?.querySelector(`.${X}-menu`);t&&(t.style.maxHeight=``),x.value=!1,T.value=-1,ae(),e&&P?.focus()},be=()=>{let e=e=>{P?.parentElement?.contains(e.target)||ye(!1)},t=()=>ve();document.addEventListener(`click`,e),window.addEventListener(`resize`,t),window.addEventListener(`scroll`,t,!0),ae=()=>{document.removeEventListener(`click`,e),window.removeEventListener(`resize`,t),window.removeEventListener(`scroll`,t,!0),ae=()=>{}}},Se=e=>{if(P=e.currentTarget,x.value){ye(!1);return}L(I())},Ce=(e,t,n=O().indexOf(e))=>{e.disabled||(t.preventDefault(),T.value=n,C.value=e.value,S.value=!1,r?.(t),ye(!0))},we=e=>{let t=T.value>=0?T.value:I(),n=he(t,e);n>=0&&(T.value=n)},Te=e=>{let t=O(),n=e>0?-1:t.length,r=he(n,e);r>=0&&(T.value=r)},Ee=e=>{let t=O();if(t.length===0)return;let n=T.value>=0?T.value:I();for(let r=1;r<=t.length;r+=1){let i=(n+r+t.length*2)%t.length,a=t[i];if(!a.disabled&&String(a.label).toLocaleLowerCase().startsWith(e)){T.value=i;return}}},De=e=>{if(P=e.currentTarget,e.key===`Escape`&&x.value){e.preventDefault(),ye(!0);return}if(e.key===`ArrowDown`||e.key===`ArrowUp`){e.preventDefault(),x.value||L(I()),we(e.key===`ArrowDown`?1:-1);return}if(e.key===`Home`||e.key===`End`){e.preventDefault(),x.value||L(I()),Te(e.key===`Home`?1:-1);return}if(e.key===`Enter`){if(e.preventDefault(),!x.value){L(I());return}let t=O()[T.value];t&&Ce(t,e,T.value);return}if(e.key===` `){e.preventDefault(),Se(e);return}e.key.length===1&&!e.altKey&&!e.ctrlKey&&!e.metaKey&&(e.preventDefault(),x.value||L(I()),Ee(e.key.toLocaleLowerCase()))},Oe=F(()=>{let e=String(C.value??``);return O().map((t,n)=>R`<button type="button" class="${X}-option" id="${le(n)}" value="${t.value}" role="option" aria-selected="${String(t.value)===e}" data-active="${n===T.value}" ?disabled=${t.disabled} .onclick=${e=>Ce(t,e,n)}>${ee(t,{location:`option`,selected:String(t.value)===e})}</button>`)}),ke=F(()=>T.value>=0?le(T.value):void 0),Ae=R`<div class="${X}-menu ${X}-menu-${w}" id="${ce}" role="listbox" aria-label="${F(()=>`${fe.value} options`)}" ?hidden=${F(()=>!x.value)}>${Oe}</div>`,je=F(()=>ie.value?R`<span id="${ue}" class="${X}-message ${M.value?`${X}-message-error`:``}" role="${M.value?`alert`:void 0}">${N}</span>`:null),Me=xe(or,{triggerId:se,required:l,disabled:c,formValue:re,invalid:S});return R`<div class="${X} ${X}-${E} ${_}" style="${me}"><button type="button" class="${X}-trigger" id="${se}" role="combobox" aria-haspopup="listbox" aria-expanded="${x}" aria-controls="${ce}" aria-activedescendant="${ke}" aria-label="${fe}" aria-describedby="${de}" aria-required="${F(()=>A()?`true`:void 0)}" aria-invalid="${F(()=>pe.value?`true`:void 0)}" ?disabled=${c} @click=${Se} @keydown=${De} @focus=${y} @blur=${b}><span class="${X}-value">${ne}</span><span class="${X}-chevron" aria-hidden="true"></span></button>${o===void 0?null:R`<input type="hidden" name="${o}" .value=${re} ?disabled=${c}>`}${je}${Me}${Ae}</div>`}function cr(e={}){let{ariaLabel:t=`Loading`,class:n=``,size:r=`medium`,tone:i=`accent`}=e,a=F(()=>U(r,`medium`)),o=F(()=>U(i,`accent`)),s=F(()=>U(t,`Loading`));return R`
    <span class="prism-spinner prism-spinner-${a.value} prism-spinner-${o.value} ${n}" role="status" aria-label="${s.value}">
      <span class="prism-spinner-ring" aria-hidden="true"></span>
    </span>
  `}var lr=`text-field`,ur=new Set([`small`,`medium`,`large`]),dr=0,fr=e=>e!=null&&e!==``;function pr(e={}){let{value:t=``,onInput:n,onChange:r,onFocus:i,onBlur:a,id:o,name:s,placeholder:c,disabled:l=!1,required:u=!1,size:d=`medium`,type:f=`text`,autocomplete:p,inputMode:m,maxLength:h,minLength:g,pattern:_,readOnly:v=!1,ariaLabel:y,ariaDescription:b,ariaDescribedBy:x,ariaInvalid:S,error:C,class:w=``,style:T}=e,E=F(()=>ur.has(String(U(d,`medium`)))?String(U(d,`medium`)):`medium`),D=F(()=>String(U(f,`text`))),O=F(()=>U(C)),ee=F(()=>fr(O.value)?O.value:U(b)),te=F(()=>fr(O.value)),ne=F(()=>fr(ee.value)),re=o?`${o}-message`:`prism-text-field-${dr+=1}-message`,k=F(()=>[U(x),ne.value?re:void 0].filter(Boolean).join(` `)||void 0),A=F(()=>{let e=U(S);return e==null?te.value:!!e}),j=F(()=>[lr,`${lr}-${E.value}`,A.value?`${lr}-invalid`:``,w].filter(Boolean).join(` `)),M=F(()=>U(T)),N=rn(t)?null:R`<input
      type="${D}"
      class="${j}"
      id="${o}"
      name="${s}"
      placeholder="${c}"
      autocomplete="${p}"
      inputmode="${m}"
      maxlength="${h}"
      minlength="${g}"
      pattern="${_}"
      aria-label="${y}"
      aria-describedby="${k}"
      aria-invalid="${F(()=>A.value?`true`:void 0)}"
      .value=${H(t)?t:String(t??``)}
      ?disabled=${l}
      ?required=${u}
      ?readonly=${v}
      style="${M}"
      @input=${n}
      @change=${r}
      @focus=${i}
      @blur=${a}
    >`;return R`${rn(t)?R`<input
      type="${D}"
      class="${j}"
      id="${o}"
      name="${s}"
      placeholder="${c}"
      autocomplete="${p}"
      inputmode="${m}"
      maxlength="${h}"
      minlength="${g}"
      pattern="${_}"
      aria-label="${y}"
      aria-describedby="${k}"
      aria-invalid="${F(()=>A.value?`true`:void 0)}"
      use:bind=${t}
      ?disabled=${l}
      ?required=${u}
      ?readonly=${v}
      style="${M}"
      @input=${n}
      @change=${r}
      @focus=${i}
      @blur=${a}
    >`:N}${F(()=>{let e=ee.value;return fr(e)?R`<span id="${re}" class="${lr}-message ${te.value?`${lr}-message-error`:``}" role="${te.value?`alert`:void 0}">${e}</span>`:null})}`}var mr=Object.freeze({cobalt:Object.freeze({"--prism-button-primary-border":`rgb(54 87 214 / 20%)`,"--prism-button-primary-background":`linear-gradient(135deg, #3657d6, #4e73ea)`,"--prism-button-primary-background-hover":`linear-gradient(135deg, #2f4dc5, #4569df)`,"--prism-button-primary-background-active":`linear-gradient(135deg, #2842ae, #3a58c7)`,"--prism-button-primary-shadow":`0 .45rem 1rem rgb(54 87 214 / 24%), inset 0 1px 0 rgb(255 255 255 / 20%)`,"--prism-button-primary-shadow-hover":`0 .6rem 1.2rem rgb(54 87 214 / 32%), inset 0 1px 0 rgb(255 255 255 / 24%)`,"--prism-button-primary-shadow-active":`0 .15rem .35rem rgb(54 87 214 / 20%), inset 0 2px 5px rgb(40 66 174 / 30%)`,"--prism-button-secondary-border":`rgb(112 128 153 / 22%)`,"--prism-button-secondary-background":`linear-gradient(135deg, #708099, #8594ab)`,"--prism-button-secondary-background-hover":`linear-gradient(135deg, #62718a, #77859b)`,"--prism-button-secondary-background-active":`linear-gradient(135deg, #556178, #677387)`,"--prism-button-secondary-shadow":`0 .36rem .9rem rgb(112 128 153 / 20%), inset 0 1px 0 rgb(255 255 255 / 16%)`,"--prism-button-secondary-shadow-hover":`0 .52rem 1.1rem rgb(112 128 153 / 26%), inset 0 1px 0 rgb(255 255 255 / 20%)`,"--prism-button-secondary-shadow-active":`0 .12rem .32rem rgb(112 128 153 / 18%), inset 0 2px 5px rgb(65 79 102 / 24%)`,"--prism-button-tertiary-border":`rgb(240 138 107 / 22%)`,"--prism-button-tertiary-background":`linear-gradient(135deg, #f08a6b, #f4a17e)`,"--prism-button-tertiary-background-hover":`linear-gradient(135deg, #e57d5d, #ef9471)`,"--prism-button-tertiary-background-active":`linear-gradient(135deg, #cf684b, #de7d5e)`,"--prism-button-tertiary-shadow":`0 .42rem .96rem rgb(240 138 107 / 22%), inset 0 1px 0 rgb(255 255 255 / 18%)`,"--prism-button-tertiary-shadow-hover":`0 .56rem 1.14rem rgb(240 138 107 / 28%), inset 0 1px 0 rgb(255 255 255 / 22%)`,"--prism-button-tertiary-shadow-active":`0 .12rem .32rem rgb(240 138 107 / 18%), inset 0 2px 5px rgb(150 80 55 / 26%)`}),iris:Object.freeze({"--prism-button-primary-border":`rgb(109 94 247 / 20%)`,"--prism-button-primary-background":`linear-gradient(135deg, #6d5ef7, #8a76ff)`,"--prism-button-primary-background-hover":`linear-gradient(135deg, #5f4eeb, #7b66f8)`,"--prism-button-primary-background-active":`linear-gradient(135deg, #5343d4, #6c57e4)`,"--prism-button-primary-shadow":`0 .45rem 1rem rgb(109 94 247 / 24%), inset 0 1px 0 rgb(255 255 255 / 20%)`,"--prism-button-primary-shadow-hover":`0 .6rem 1.2rem rgb(109 94 247 / 32%), inset 0 1px 0 rgb(255 255 255 / 24%)`,"--prism-button-primary-shadow-active":`0 .15rem .35rem rgb(109 94 247 / 20%), inset 0 2px 5px rgb(65 52 165 / 30%)`,"--prism-button-secondary-border":`rgb(100 116 139 / 22%)`,"--prism-button-secondary-background":`linear-gradient(135deg, #64748b, #7a889c)`,"--prism-button-secondary-background-hover":`linear-gradient(135deg, #56657b, #6c7a8e)`,"--prism-button-secondary-background-active":`linear-gradient(135deg, #4a576b, #5e6c80)`,"--prism-button-secondary-shadow":`0 .36rem .9rem rgb(100 116 139 / 20%), inset 0 1px 0 rgb(255 255 255 / 16%)`,"--prism-button-secondary-shadow-hover":`0 .52rem 1.1rem rgb(100 116 139 / 26%), inset 0 1px 0 rgb(255 255 255 / 20%)`,"--prism-button-secondary-shadow-active":`0 .12rem .32rem rgb(100 116 139 / 18%), inset 0 2px 5px rgb(51 65 85 / 24%)`,"--prism-button-tertiary-border":`rgb(242 107 94 / 22%)`,"--prism-button-tertiary-background":`linear-gradient(135deg, #f26b5e, #f58a74)`,"--prism-button-tertiary-background-hover":`linear-gradient(135deg, #e66054, #ee7d68)`,"--prism-button-tertiary-background-active":`linear-gradient(135deg, #cf554b, #dd6d5b)`,"--prism-button-tertiary-shadow":`0 .42rem .96rem rgb(242 107 94 / 22%), inset 0 1px 0 rgb(255 255 255 / 18%)`,"--prism-button-tertiary-shadow-hover":`0 .56rem 1.14rem rgb(242 107 94 / 28%), inset 0 1px 0 rgb(255 255 255 / 22%)`,"--prism-button-tertiary-shadow-active":`0 .12rem .32rem rgb(242 107 94 / 18%), inset 0 2px 5px rgb(145 64 50 / 26%)`}),teal:Object.freeze({"--prism-button-primary-border":`rgb(15 118 110 / 22%)`,"--prism-button-primary-background":`linear-gradient(135deg, #0f766e, #159b91)`,"--prism-button-primary-background-hover":`linear-gradient(135deg, #0d675f, #11867f)`,"--prism-button-primary-background-active":`linear-gradient(135deg, #0b5852, #0e726b)`,"--prism-button-primary-shadow":`0 .45rem 1rem rgb(15 118 110 / 24%), inset 0 1px 0 rgb(255 255 255 / 20%)`,"--prism-button-primary-shadow-hover":`0 .6rem 1.2rem rgb(15 118 110 / 32%), inset 0 1px 0 rgb(255 255 255 / 24%)`,"--prism-button-primary-shadow-active":`0 .15rem .35rem rgb(15 118 110 / 20%), inset 0 2px 5px rgb(12 78 74 / 30%)`,"--prism-button-secondary-border":`rgb(107 124 147 / 22%)`,"--prism-button-secondary-background":`linear-gradient(135deg, #6b7c93, #8190a5)`,"--prism-button-secondary-background-hover":`linear-gradient(135deg, #5e7087, #74849a)`,"--prism-button-secondary-background-active":`linear-gradient(135deg, #526278, #66768d)`,"--prism-button-secondary-shadow":`0 .36rem .9rem rgb(107 124 147 / 20%), inset 0 1px 0 rgb(255 255 255 / 16%)`,"--prism-button-secondary-shadow-hover":`0 .52rem 1.1rem rgb(107 124 147 / 26%), inset 0 1px 0 rgb(255 255 255 / 20%)`,"--prism-button-secondary-shadow-active":`0 .12rem .32rem rgb(107 124 147 / 18%), inset 0 2px 5px rgb(57 69 86 / 24%)`,"--prism-button-tertiary-border":`rgb(245 158 11 / 22%)`,"--prism-button-tertiary-background":`linear-gradient(135deg, #f59e0b, #f7b84a)`,"--prism-button-tertiary-background-hover":`linear-gradient(135deg, #e39107, #efae36)`,"--prism-button-tertiary-background-active":`linear-gradient(135deg, #c97c06, #d89522)`,"--prism-button-tertiary-shadow":`0 .42rem .96rem rgb(245 158 11 / 22%), inset 0 1px 0 rgb(255 255 255 / 18%)`,"--prism-button-tertiary-shadow-hover":`0 .56rem 1.14rem rgb(245 158 11 / 28%), inset 0 1px 0 rgb(255 255 255 / 22%)`,"--prism-button-tertiary-shadow-active":`0 .12rem .32rem rgb(245 158 11 / 18%), inset 0 2px 5px rgb(146 94 20 / 26%)`})}),Z=Object.freeze({colors:Object.freeze({page:`#f5f7fb`,surface:`#ffffff`,surfaceGlass:`rgb(255 255 255 / 84%)`,surfaceCard:`rgb(255 255 255 / 82%)`,surfaceTint:`#f8faff`,surfaceRaised:`#ffffff`,surfaceHover:`#f8faff`,white:`#ffffff`,whiteStrong:`rgb(255 255 255 / 90%)`,whiteSoft:`rgb(255 255 255 / 24%)`,whiteFaint:`rgb(255 255 255 / 20%)`,whiteTint:`rgb(255 255 255 / 12%)`,ink:`#1d2638`,text:`#53617a`,textMuted:`#6b7892`,textSoft:`#7b879e`,textSubtle:`#8a95a8`,placeholder:`#aab3c2`,border:`#e5e9f1`,borderStrong:`#cbd3e1`,borderInput:`#dbe1eb`,borderFaint:`#edf0f5`,accent:`#ef685a`,accentBright:`#f26b5e`,accentGlow:`rgb(242 107 94 / 12%)`,accentSoft:`#fff0ed`,accentHover:`#dc594c`,action:`#3657d6`,actionEnd:`#4e73ea`,actionHover:`#2f4dc5`,actionHoverEnd:`#4569df`,actionActive:`#2842ae`,actionActiveEnd:`#3a58c7`,primary:`#3657d6`,primaryEnd:`#4e73ea`,primaryHover:`#2f4dc5`,primaryHoverEnd:`#4569df`,primaryActive:`#2842ae`,primaryActiveEnd:`#3a58c7`,secondary:`#708099`,secondaryEnd:`#8594ab`,secondaryHover:`#62718a`,secondaryHoverEnd:`#77859b`,secondaryActive:`#556178`,secondaryActiveEnd:`#677387`,tertiary:`#f08a6b`,tertiaryEnd:`#f4a17e`,tertiaryHover:`#e57d5d`,tertiaryHoverEnd:`#ef9471`,tertiaryActive:`#cf684b`,tertiaryActiveEnd:`#de7d5e`,error:`#dc2626`,errorEnd:`#ef4444`,errorHover:`#c81f1f`,errorHoverEnd:`#df3b3b`,errorActive:`#b91c1c`,errorActiveEnd:`#cd3030`,warning:`#d97706`,warningEnd:`#f59e0b`,warningHover:`#c56b05`,warningHoverEnd:`#e68f08`,warningActive:`#a95b05`,warningActiveEnd:`#c97908`,information:`#0284c7`,informationEnd:`#0ea5e9`,informationHover:`#036fa8`,informationHoverEnd:`#0b92d0`,informationActive:`#075985`,informationActiveEnd:`#0478b1`,actionPreview:`#7787a4`,actionPreviewHover:`#657593`,actionGlow:`rgb(54 87 214 / 24%)`,actionHoverGlow:`rgb(54 87 214 / 32%)`,actionActiveShadow:`rgb(40 66 174 / 30%)`,actionFocusGlow:`rgb(54 87 214 / 24%)`,focus:`#4e73ea`,focusGlow:`rgb(78 115 234 / 14%)`,focusStrongGlow:`rgb(78 115 234 / 20%)`,invalidGlow:`rgb(239 104 90 / 12%)`,success:`#3c9b7a`,successBright:`#53c69d`,successGlow:`rgb(83 198 157 / 15%)`,previewGlow:`rgb(166 142 241 / 12%)`,lavenderBorder:`#d9d1ff`,lavenderSurface:`#f0edff`,mintBorder:`#c7ebdf`,mintSurface:`#eaf9f4`,peachBorder:`#f5d5c9`,peachSurface:`#fff1eb`}),fontSizes:Object.freeze({micro:`.7rem`,label:`.72rem`,small:`.75rem`,bodySmall:`.78rem`,compact:`.76rem`,body:`.82rem`,copy:`.9rem`,cardCopy:`.92rem`,ui:`.8rem`,lead:`1.05rem`,heading:`1.45rem`,hero:`clamp(3rem, 7vw, 5.8rem)`,detailHero:`clamp(3rem, 7vw, 5rem)`}),radii:Object.freeze({control:`.6rem`,card:`1.25rem`,preview:`.85rem`,surface:`1rem`}),shadows:Object.freeze({card:`0 .9rem 2.5rem rgb(37 49 78 / 6%)`,action:`0 .45rem 1rem rgb(54 87 214 / 24%), inset 0 1px 0 rgb(255 255 255 / 20%)`,actionHover:`0 .6rem 1.2rem rgb(54 87 214 / 32%), inset 0 1px 0 rgb(255 255 255 / 24%)`,actionActive:`0 .15rem .35rem rgb(54 87 214 / 20%), inset 0 2px 5px rgb(40 66 174 / 30%)`,secondary:`0 .36rem .9rem rgb(112 128 153 / 20%), inset 0 1px 0 rgb(255 255 255 / 16%)`,secondaryHover:`0 .52rem 1.1rem rgb(112 128 153 / 26%), inset 0 1px 0 rgb(255 255 255 / 20%)`,secondaryActive:`0 .12rem .32rem rgb(112 128 153 / 18%), inset 0 2px 5px rgb(65 79 102 / 24%)`,tertiary:`0 .42rem .96rem rgb(240 138 107 / 22%), inset 0 1px 0 rgb(255 255 255 / 18%)`,tertiaryHover:`0 .56rem 1.14rem rgb(240 138 107 / 28%), inset 0 1px 0 rgb(255 255 255 / 22%)`,tertiaryActive:`0 .12rem .32rem rgb(240 138 107 / 18%), inset 0 2px 5px rgb(150 80 55 / 26%)`,error:`0 .4rem .92rem rgb(220 38 38 / 22%), inset 0 1px 0 rgb(255 255 255 / 16%)`,errorHover:`0 .54rem 1.08rem rgb(220 38 38 / 28%), inset 0 1px 0 rgb(255 255 255 / 22%)`,errorActive:`0 .12rem .32rem rgb(220 38 38 / 18%), inset 0 2px 5px rgb(127 29 29 / 26%)`,warning:`0 .4rem .92rem rgb(217 119 6 / 22%), inset 0 1px 0 rgb(255 255 255 / 16%)`,warningHover:`0 .54rem 1.08rem rgb(217 119 6 / 28%), inset 0 1px 0 rgb(255 255 255 / 22%)`,warningActive:`0 .12rem .32rem rgb(217 119 6 / 18%), inset 0 2px 5px rgb(120 53 15 / 26%)`,information:`0 .4rem .92rem rgb(2 132 199 / 22%), inset 0 1px 0 rgb(255 255 255 / 16%)`,informationHover:`0 .54rem 1.08rem rgb(2 132 199 / 28%), inset 0 1px 0 rgb(255 255 255 / 22%)`,informationActive:`0 .12rem .32rem rgb(2 132 199 / 18%), inset 0 2px 5px rgb(12 74 110 / 26%)`,success:`0 .4rem .92rem rgb(60 155 122 / 22%), inset 0 1px 0 rgb(255 255 255 / 16%)`,successHover:`0 .54rem 1.08rem rgb(60 155 122 / 28%), inset 0 1px 0 rgb(255 255 255 / 22%)`,successActive:`0 .12rem .32rem rgb(60 155 122 / 18%), inset 0 2px 5px rgb(24 101 73 / 26%)`})}),hr=Object.freeze({"--prism-button-padding":`.6rem .8rem`,"--prism-button-border-width":`1px`,"--prism-button-radius":Z.radii.control,"--prism-button-font-size":Z.fontSizes.small,"--prism-button-font-weight":`750`,"--prism-button-transform-hover":`translateY(-1px)`,"--prism-button-transform-active":`translateY(1px) scale(.98)`,"--prism-button-focus-outline":`3px solid ${Z.colors.actionFocusGlow}`,"--prism-button-focus-offset":`3px`,"--prism-button-disabled-opacity":`.55`,"--prism-button-primary-text":Z.colors.white,"--prism-button-primary-border":Z.colors.whiteSoft,"--prism-button-primary-background":`linear-gradient(135deg, ${Z.colors.primary}, ${Z.colors.primaryEnd})`,"--prism-button-primary-background-hover":`linear-gradient(135deg, ${Z.colors.primaryHover}, ${Z.colors.primaryHoverEnd})`,"--prism-button-primary-background-active":`linear-gradient(135deg, ${Z.colors.primaryActive}, ${Z.colors.primaryActiveEnd})`,"--prism-button-primary-shadow":Z.shadows.action,"--prism-button-primary-shadow-hover":Z.shadows.actionHover,"--prism-button-primary-shadow-active":Z.shadows.actionActive,"--prism-button-secondary-text":Z.colors.white,"--prism-button-secondary-border":`rgb(100 116 139 / 22%)`,"--prism-button-secondary-background":`linear-gradient(135deg, ${Z.colors.secondary}, ${Z.colors.secondaryEnd})`,"--prism-button-secondary-background-hover":`linear-gradient(135deg, ${Z.colors.secondaryHover}, ${Z.colors.secondaryHoverEnd})`,"--prism-button-secondary-background-active":`linear-gradient(135deg, ${Z.colors.secondaryActive}, ${Z.colors.secondaryActiveEnd})`,"--prism-button-secondary-shadow":Z.shadows.secondary,"--prism-button-secondary-shadow-hover":Z.shadows.secondaryHover,"--prism-button-secondary-shadow-active":Z.shadows.secondaryActive,"--prism-button-tertiary-text":Z.colors.white,"--prism-button-tertiary-border":`rgb(231 111 81 / 22%)`,"--prism-button-tertiary-background":`linear-gradient(135deg, ${Z.colors.tertiary}, ${Z.colors.tertiaryEnd})`,"--prism-button-tertiary-background-hover":`linear-gradient(135deg, ${Z.colors.tertiaryHover}, ${Z.colors.tertiaryHoverEnd})`,"--prism-button-tertiary-background-active":`linear-gradient(135deg, ${Z.colors.tertiaryActive}, ${Z.colors.tertiaryActiveEnd})`,"--prism-button-tertiary-shadow":Z.shadows.tertiary,"--prism-button-tertiary-shadow-hover":Z.shadows.tertiaryHover,"--prism-button-tertiary-shadow-active":Z.shadows.tertiaryActive,"--prism-button-error-text":Z.colors.white,"--prism-button-error-border":`rgb(220 38 38 / 22%)`,"--prism-button-error-background":`linear-gradient(135deg, ${Z.colors.error}, ${Z.colors.errorEnd})`,"--prism-button-error-background-hover":`linear-gradient(135deg, ${Z.colors.errorHover}, ${Z.colors.errorHoverEnd})`,"--prism-button-error-background-active":`linear-gradient(135deg, ${Z.colors.errorActive}, ${Z.colors.errorActiveEnd})`,"--prism-button-error-shadow":Z.shadows.error,"--prism-button-error-shadow-hover":Z.shadows.errorHover,"--prism-button-error-shadow-active":Z.shadows.errorActive,"--prism-button-warning-text":Z.colors.white,"--prism-button-warning-border":`rgb(217 119 6 / 22%)`,"--prism-button-warning-background":`linear-gradient(135deg, ${Z.colors.warning}, ${Z.colors.warningEnd})`,"--prism-button-warning-background-hover":`linear-gradient(135deg, ${Z.colors.warningHover}, ${Z.colors.warningHoverEnd})`,"--prism-button-warning-background-active":`linear-gradient(135deg, ${Z.colors.warningActive}, ${Z.colors.warningActiveEnd})`,"--prism-button-warning-shadow":Z.shadows.warning,"--prism-button-warning-shadow-hover":Z.shadows.warningHover,"--prism-button-warning-shadow-active":Z.shadows.warningActive,"--prism-button-information-text":Z.colors.white,"--prism-button-information-border":`rgb(2 132 199 / 22%)`,"--prism-button-information-background":`linear-gradient(135deg, ${Z.colors.information}, ${Z.colors.informationEnd})`,"--prism-button-information-background-hover":`linear-gradient(135deg, ${Z.colors.informationHover}, ${Z.colors.informationHoverEnd})`,"--prism-button-information-background-active":`linear-gradient(135deg, ${Z.colors.informationActive}, ${Z.colors.informationActiveEnd})`,"--prism-button-information-shadow":Z.shadows.information,"--prism-button-information-shadow-hover":Z.shadows.informationHover,"--prism-button-information-shadow-active":Z.shadows.informationActive,"--prism-button-success-text":Z.colors.white,"--prism-button-success-border":`rgb(60 155 122 / 22%)`,"--prism-button-success-background":`linear-gradient(135deg, ${Z.colors.success}, ${Z.colors.successBright})`,"--prism-button-success-background-hover":`linear-gradient(135deg, #318b6d, ${Z.colors.success})`,"--prism-button-success-background-active":`linear-gradient(135deg, #27765c, #3b8d71)`,"--prism-button-success-shadow":Z.shadows.success,"--prism-button-success-shadow-hover":Z.shadows.successHover,"--prism-button-success-shadow-active":Z.shadows.successActive,"--prism-color-page":Z.colors.page,"--prism-color-surface":Z.colors.surface,"--prism-color-surface-glass":Z.colors.surfaceGlass,"--prism-color-surface-card":Z.colors.surfaceCard,"--prism-color-surface-tint":Z.colors.surfaceTint,"--prism-color-surface-raised":Z.colors.surfaceRaised,"--prism-color-surface-hover":Z.colors.surfaceHover,"--prism-color-white":Z.colors.white,"--prism-color-white-strong":Z.colors.whiteStrong,"--prism-color-white-soft":Z.colors.whiteSoft,"--prism-color-white-faint":Z.colors.whiteFaint,"--prism-color-white-tint":Z.colors.whiteTint,"--prism-color-ink":Z.colors.ink,"--prism-color-text":Z.colors.text,"--prism-color-text-muted":Z.colors.textMuted,"--prism-color-text-soft":Z.colors.textSoft,"--prism-color-text-subtle":Z.colors.textSubtle,"--prism-color-placeholder":Z.colors.placeholder,"--prism-color-border":Z.colors.border,"--prism-color-border-strong":Z.colors.borderStrong,"--prism-color-border-input":Z.colors.borderInput,"--prism-color-border-faint":Z.colors.borderFaint,"--prism-color-accent":Z.colors.accent,"--prism-color-accent-bright":Z.colors.accentBright,"--prism-color-accent-glow":Z.colors.accentGlow,"--prism-color-accent-soft":Z.colors.accentSoft,"--prism-color-accent-hover":Z.colors.accentHover,"--prism-color-action":Z.colors.action,"--prism-color-action-end":Z.colors.actionEnd,"--prism-color-action-hover":Z.colors.actionHover,"--prism-color-action-hover-end":Z.colors.actionHoverEnd,"--prism-color-action-active":Z.colors.actionActive,"--prism-color-action-active-end":Z.colors.actionActiveEnd,"--prism-color-primary":Z.colors.primary,"--prism-color-primary-end":Z.colors.primaryEnd,"--prism-color-primary-hover":Z.colors.primaryHover,"--prism-color-primary-hover-end":Z.colors.primaryHoverEnd,"--prism-color-primary-active":Z.colors.primaryActive,"--prism-color-primary-active-end":Z.colors.primaryActiveEnd,"--prism-color-secondary":Z.colors.secondary,"--prism-color-secondary-end":Z.colors.secondaryEnd,"--prism-color-secondary-hover":Z.colors.secondaryHover,"--prism-color-secondary-hover-end":Z.colors.secondaryHoverEnd,"--prism-color-secondary-active":Z.colors.secondaryActive,"--prism-color-secondary-active-end":Z.colors.secondaryActiveEnd,"--prism-color-tertiary":Z.colors.tertiary,"--prism-color-tertiary-end":Z.colors.tertiaryEnd,"--prism-color-tertiary-hover":Z.colors.tertiaryHover,"--prism-color-tertiary-hover-end":Z.colors.tertiaryHoverEnd,"--prism-color-tertiary-active":Z.colors.tertiaryActive,"--prism-color-tertiary-active-end":Z.colors.tertiaryActiveEnd,"--prism-color-error":Z.colors.error,"--prism-color-error-end":Z.colors.errorEnd,"--prism-color-warning":Z.colors.warning,"--prism-color-warning-end":Z.colors.warningEnd,"--prism-color-information":Z.colors.information,"--prism-color-information-end":Z.colors.informationEnd,"--prism-color-action-preview":Z.colors.actionPreview,"--prism-color-action-preview-hover":Z.colors.actionPreviewHover,"--prism-color-action-glow":Z.colors.actionGlow,"--prism-color-action-hover-glow":Z.colors.actionHoverGlow,"--prism-color-action-active-shadow":Z.colors.actionActiveShadow,"--prism-color-action-focus-glow":Z.colors.actionFocusGlow,"--prism-color-focus":Z.colors.focus,"--prism-color-focus-glow":Z.colors.focusGlow,"--prism-color-focus-strong-glow":Z.colors.focusStrongGlow,"--prism-color-invalid-glow":Z.colors.invalidGlow,"--prism-color-success":Z.colors.success,"--prism-color-success-bright":Z.colors.successBright,"--prism-color-success-glow":Z.colors.successGlow,"--prism-color-preview-glow":Z.colors.previewGlow,"--prism-color-lavender-border":Z.colors.lavenderBorder,"--prism-color-lavender-surface":Z.colors.lavenderSurface,"--prism-color-mint-border":Z.colors.mintBorder,"--prism-color-mint-surface":Z.colors.mintSurface,"--prism-color-peach-border":Z.colors.peachBorder,"--prism-color-peach-surface":Z.colors.peachSurface,"--prism-font-size-micro":Z.fontSizes.micro,"--prism-font-size-label":Z.fontSizes.label,"--prism-font-size-small":Z.fontSizes.small,"--prism-font-size-body-small":Z.fontSizes.bodySmall,"--prism-font-size-compact":Z.fontSizes.compact,"--prism-font-size-body":Z.fontSizes.body,"--prism-font-size-copy":Z.fontSizes.copy,"--prism-font-size-card-copy":Z.fontSizes.cardCopy,"--prism-font-size-ui":Z.fontSizes.ui,"--prism-font-size-lead":Z.fontSizes.lead,"--prism-font-size-heading":Z.fontSizes.heading,"--prism-font-size-hero":Z.fontSizes.hero,"--prism-font-size-detail-hero":Z.fontSizes.detailHero,"--prism-radius-control":Z.radii.control,"--prism-radius-card":Z.radii.card,"--prism-radius-preview":Z.radii.preview,"--prism-radius-surface":Z.radii.surface,"--prism-shadow-card":Z.shadows.card,"--prism-shadow-action":Z.shadows.action,"--prism-shadow-action-hover":Z.shadows.actionHover,"--prism-shadow-action-active":Z.shadows.actionActive}),gr=Ze(`
  :root {
    color-scheme: light;
    ${Object.entries(hr).map(([e,t])=>`${e}: ${t};`).join(`
    `)}
  }

  ${Object.entries(mr).map(([e,t])=>`
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
`),_r=[{name:`Session`,endpoints:[{name:`Current account`,method:`GET`,path:`/v1/auth/me`,auth:`bearer`},{name:`Log out`,method:`POST`,path:`/v1/auth/logout`,auth:`bearer`},{name:`Revoke other sessions`,method:`DELETE`,path:`/v1/auth/sessions/others`,auth:`bearer`}]},{name:`Username auth`,endpoints:[{name:`Create challenge`,method:`POST`,path:`/v1/auth/username/challenge`,auth:`none`,body:{handle:`alice`,purpose:`login`,device_id:`<device UUID>`,mls_node_id:`<MLS node UUID>`,public_key:`<base64url public key>`}},{name:`Register`,method:`POST`,path:`/v1/auth/username/register`,auth:`none`,body:{challenge_id:`<challenge UUID>`,signature:`<base64url signature>`}},{name:`Log in`,method:`POST`,path:`/v1/auth/username/login`,auth:`none`,body:{challenge_id:`<challenge UUID>`,signature:`<base64url signature>`}}]},{name:`Account profile`,endpoints:[{name:`Read display name`,method:`GET`,path:`/v1/account/display-name`,auth:`bearer`},{name:`Change display name`,method:`PUT`,path:`/v1/account/display-name`,auth:`bearer`,body:{display_name:`Ava`}}]},{name:`Directory`,endpoints:[{name:`Find by username`,method:`GET`,path:`/v1/directory/alice`,auth:`none`},{name:`Find by user ID`,method:`GET`,path:`/v1/directory/users/<user UUID>`,auth:`bearer`},{name:`Sync saved contact profiles`,method:`POST`,path:`/v1/directory/profiles/sync`,auth:`bearer`,body:{user_ids:[`<user UUID>`]}}]},{name:`Devices and keys`,endpoints:[{name:`Register device`,method:`POST`,path:`/v1/devices`,auth:`bearer`,body:{}},{name:`Pre-key inventory`,method:`GET`,path:`/v1/prekeys/status`,auth:`bearer`},{name:`Upload pre-keys`,method:`PUT`,path:`/v1/prekeys`,auth:`bearer`,body:{}},{name:`Download MLS key package`,method:`GET`,path:`/v1/mls/key-package/<device UUID>`,auth:`bearer`}]},{name:`Groups`,endpoints:[{name:`Create group`,method:`POST`,path:`/v1/groups`,auth:`bearer`,body:{}},{name:`List members`,method:`GET`,path:`/v1/groups/<group UUID>/members`,auth:`bearer`}]},{name:`Admin`,endpoints:[{name:`List users`,method:`GET`,path:`/v1/admin/users`,auth:`admin`},{name:`Set user status`,method:`PUT`,path:`/v1/admin/users/<user UUID>/status`,auth:`admin`,body:{disabled:!0}},{name:`Delete user`,method:`DELETE`,path:`/v1/admin/users/<user UUID>`,auth:`admin`}]}],vr=`links-http-client-history-v1`,yr=[`GET`,`POST`,`PUT`,`PATCH`,`DELETE`].map(e=>({value:e,label:e})),br=[{value:`none`,label:`No authentication`},{value:`bearer`,label:`Bearer token`},{value:`admin`,label:`Admin key`}],xr=k(`/links-api`),Q=k(`GET`),Sr=k(`/v1/auth/me`),$=k(`bearer`),Cr=k(``),wr=k(``),Tr=k(`{}`),Er=k(``),Dr=k(null),Or=k(``),kr=k(!1),Ar=k(!1),jr=k(Nr()),Mr=null;function Nr(){try{let e=JSON.parse(localStorage.getItem(vr)||`[]`);return Array.isArray(e)?e.slice(0,20):[]}catch{return[]}}function Pr(e){jr.value=e.slice(0,20);try{localStorage.setItem(vr,JSON.stringify(jr.value))}catch{}}function Fr(e){return JSON.stringify(e,null,2)}function Ir(){if(!Tr.value.trim())return{};let e=JSON.parse(Tr.value);if(!e||Array.isArray(e)||typeof e!=`object`)throw Error(`Headers must be a JSON object.`);return Object.fromEntries(Object.entries(e).map(([e,t])=>[e,String(t)]))}function Lr(){let e=Sr.value.trim();return/^https?:\/\//i.test(e)?e:`${xr.value.trim().replace(/\/$/,``)}${e.startsWith(`/`)?e:`/${e}`}`}function Rr(e){Q.value=e.method,Sr.value=e.path,$.value=e.auth,Er.value=e.body?Fr(e.body):``,Dr.value=null,Or.value=``}async function zr(e){if(e?.preventDefault(),kr.value)return;Or.value=``,Ar.value=!1;let t;try{t=Ir()}catch(e){Or.value=e.message;return}$.value===`bearer`&&Cr.value.trim()&&(t.Authorization=`Bearer ${Cr.value.trim()}`),$.value===`admin`&&wr.value.trim()&&(t[`X-Links-Admin-Key`]=wr.value.trim());let n=![`GET`,`HEAD`].includes(Q.value)&&Er.value.trim().length>0;n&&!Object.keys(t).some(e=>e.toLowerCase()===`content-type`)&&(t[`Content-Type`]=`application/json`),Mr=new AbortController,kr.value=!0;let r=performance.now(),i=Lr();try{let e=await fetch(i,{method:Q.value,headers:t,body:n?Er.value:void 0,signal:Mr.signal}),a=Math.round(performance.now()-r),o=await e.text(),s=e.headers.get(`content-type`)||``,c=o,l=`text`;if(s.includes(`json`)||/^[\s]*[\[{]/.test(o))try{c=Fr(JSON.parse(o)),l=`json`}catch{l=`text`}Dr.value={ok:e.ok,status:e.status,statusText:e.statusText,elapsedMs:a,size:new Blob([o]).size,body:c||`(empty response body)`,language:l,headers:Fr(Object.fromEntries(e.headers.entries())),url:i},Pr([{id:crypto.randomUUID(),method:Q.value,path:Sr.value,auth:$.value,status:e.status,elapsedMs:a,requestedAt:new Date().toISOString()},...jr.value])}catch(e){let t=Math.round(performance.now()-r);Or.value=e.name===`AbortError`?`Request cancelled.`:`${e.message} Use /links-api during local development, or enable CORS on a direct endpoint.`,Pr([{id:crypto.randomUUID(),method:Q.value,path:Sr.value,auth:$.value,status:`ERR`,elapsedMs:t,requestedAt:new Date().toISOString()},...jr.value])}finally{kr.value=!1,Mr=null}}function Br(){Mr?.abort()}function Vr(e){Q.value=e.method,Sr.value=e.path,$.value=e.auth,Or.value=``}function Hr(){Pr([])}function Ur(e){return`'${String(e).replaceAll(`'`,`'\\''`)}'`}async function Wr(){let e;try{e=Ir()}catch(e){Or.value=e.message;return}$.value===`bearer`&&Cr.value.trim()&&(e.Authorization=`Bearer ${Cr.value.trim()}`),$.value===`admin`&&wr.value.trim()&&(e[`X-Links-Admin-Key`]=wr.value.trim());let t=![`GET`,`HEAD`].includes(Q.value)&&Er.value.trim();t&&!Object.keys(e).some(e=>e.toLowerCase()===`content-type`)&&(e[`Content-Type`]=`application/json`);let n=[`curl -i -X ${Q.value}`,Ur(Lr())];for(let[t,r]of Object.entries(e))n.push(`-H ${Ur(`${t}: ${r}`)}`);t&&n.push(`--data-raw ${Ur(Er.value)}`);try{await navigator.clipboard.writeText(n.join(` \\
  `)),Ar.value=!0,window.setTimeout(()=>{Ar.value=!1},1600)}catch{Or.value=`The browser blocked clipboard access.`}}function Gr(e){return e===`GET`?`method-get`:e===`POST`?`method-post`:e===`DELETE`?`method-delete`:`method-write`}function Kr(e){return e<1024?`${e} B`:`${(e/1024).toFixed(1)} KB`}var qr=F(()=>$.value===`bearer`?B(er,{label:`Bearer token`,hint:`Kept in memory and omitted from request history.`,control:e=>B(pr,{...e,type:`password`,value:Cr,autocomplete:`off`,placeholder:`Access token`})}):$.value===`admin`?B(er,{label:`Admin key`,hint:`Sent as X-Links-Admin-Key and never persisted.`,control:e=>B(pr,{...e,type:`password`,value:wr,autocomplete:`off`,placeholder:`Admin API key`})}):B(`p`,{class:`quiet-note`,children:`This request will not include credentials.`})),Jr=F(()=>{if(kr.value)return V(`div`,{class:`response-empty`,children:[B(cr,{size:`medium`,ariaLabel:`Sending request`}),B(`span`,{children:`Waiting for the Links service…`})]});if(!Dr.value)return B(Zn,{icon:B(mn,{size:`1.4rem`}),title:`No response yet`,description:`Choose an endpoint or compose a request, then send it.`});let e=Dr.value;return V(`section`,{class:`response-result`,"aria-live":`polite`,children:[V(`div`,{class:`response-summary`,children:[B(Cn,{value:`${e.status} ${e.statusText}`,tone:e.ok?`success`:`error`}),V(`span`,{children:[B(pn,{size:`0.9rem`}),` `,e.elapsedMs,` ms`]}),B(`span`,{children:Kr(e.size)}),B(`span`,{class:`response-url`,title:e.url,children:e.url})]}),B(Xn,{tabs:[{id:`body`,label:`Body`,language:e.language,code:e.body},{id:`headers`,label:`Headers`,language:`json`,code:e.headers}],defaultTab:`body`,lineNumbers:!0,copyable:!0,minHeight:`18rem`,maxHeight:`32rem`,ariaLabel:`HTTP response`})]})}),Yr=F(()=>jr.value.length===0?B(`p`,{class:`history-empty`,children:`Sent requests appear here.`}):jr.value.map(e=>V(`button`,{class:`history-item`,type:`button`,onClick:()=>Vr(e),children:[B(`span`,{class:`method-dot ${Gr(e.method)}`,children:e.method}),B(`span`,{class:`history-path`,children:e.path}),B(`span`,{class:e.status===`ERR`||Number(e.status)>=400?`history-status is-error`:`history-status`,children:e.status})]})));function Xr(){return V(`div`,{class:`app`,"use:style":gr,children:[V(`header`,{class:`topbar`,children:[V(`div`,{class:`brand`,children:[B(`span`,{class:`brand-mark`,children:B(cn,{size:`1.05rem`})}),V(`div`,{children:[B(`strong`,{children:`Links HTTP`}),B(`span`,{children:`Protocol workbench`})]})]}),V(`div`,{class:`endpoint-control`,children:[B(`span`,{class:`endpoint-light`,"aria-hidden":`true`}),B(pr,{value:xr,ariaLabel:`Base URL`,size:`small`}),B(`span`,{class:`proxy-label`,children:`Vite proxy`})]})]}),V(`div`,{class:`workspace`,children:[V(`aside`,{class:`catalog-pane`,children:[V(`div`,{class:`pane-heading`,children:[V(`div`,{children:[B(`h2`,{children:`Endpoints`}),B(`p`,{children:`Account service`})]}),B(Cn,{value:_r.reduce((e,t)=>e+t.endpoints.length,0)})]}),B(`nav`,{"aria-label":`Links endpoints`,class:`endpoint-list`,children:_r.map(e=>V(`section`,{class:`endpoint-group`,children:[B(`h3`,{children:e.name}),e.endpoints.map(e=>V(`button`,{type:`button`,class:`endpoint-item`,onClick:()=>Rr(e),children:[B(`span`,{class:`method-label ${Gr(e.method)}`,children:e.method}),B(`span`,{children:e.name}),e.auth===`none`?null:B(dn,{size:`0.75rem`})]}))]}))})]}),V(`main`,{class:`request-pane`,children:[V(`form`,{onSubmit:zr,children:[V(`div`,{class:`request-line`,children:[B(sr,{value:Q,options:yr,ariaLabel:`HTTP method`,class:`method-select`}),B(pr,{value:Sr,ariaLabel:`Request path`,class:`path-field`,autocomplete:`off`}),B(kn,{type:`submit`,label:`Send`,icon:B(ln,{}),iconPosition:`end`,variant:`primary`,loading:kr,loadingLabel:`Sending`}),F(()=>kr.value?B(kn,{type:`button`,label:`Cancel`,variant:`secondary`,onClick:Br}):null)]}),V(`div`,{class:`request-grid`,children:[V(`section`,{class:`request-section`,children:[V(`div`,{class:`section-heading`,children:[V(`div`,{children:[B(`h2`,{children:`Authentication`}),B(`p`,{children:`Credentials stay in this browser tab.`})]}),B(sr,{value:$,options:br,ariaLabel:`Authentication mode`,size:`small`})]}),B(`div`,{class:`auth-field`,children:qr})]}),V(`section`,{class:`request-section`,children:[B(`div`,{class:`section-heading`,children:V(`div`,{children:[B(`h2`,{children:`Headers`}),B(`p`,{children:`JSON object; generated auth headers are merged in.`})]})}),B(Xn,{code:Tr,language:`json`,filename:`headers.json`,editable:!0,lineNumbers:!0,minHeight:`8rem`,maxHeight:`13rem`,ariaLabel:`Request headers`})]})]}),V(`section`,{class:`request-section body-section`,children:[V(`div`,{class:`section-heading`,children:[V(`div`,{children:[B(`h2`,{children:`Request body`}),B(`p`,{children:`Sent exactly as written for methods that accept a body.`})]}),B(kn,{type:`button`,label:F(()=>Ar.value?`Copied`:`Copy cURL`),icon:F(()=>Ar.value?B(un,{}):B(fn,{})),variant:`tertiary`,size:`small`,onClick:Wr})]}),B(Xn,{code:Er,language:`json`,filename:`body.json`,editable:!0,lineNumbers:!0,minHeight:`12rem`,maxHeight:`24rem`,ariaLabel:`Request body`})]})]}),F(()=>Or.value?B(_n,{tone:`error`,title:`Request failed`,children:Or}):null),V(`section`,{class:`response-section`,children:[B(`div`,{class:`pane-heading response-heading`,children:V(`div`,{children:[B(`h2`,{children:`Response`}),B(`p`,{children:`Status, headers and payload`})]})}),Jr]})]}),V(`aside`,{class:`history-pane`,children:[V(`div`,{class:`pane-heading`,children:[V(`div`,{children:[B(`h2`,{children:`History`}),B(`p`,{children:`Metadata only`})]}),B(kn,{type:`button`,label:`Clear`,showLabel:!1,icon:B(sn,{}),ariaLabel:`Clear request history`,variant:`tertiary`,size:`small`,onClick:Hr})]}),B(`div`,{class:`history-list`,children:Yr}),V(`div`,{class:`privacy-note`,children:[B(dn,{size:`0.85rem`}),B(`span`,{children:`Tokens, keys, headers and bodies are never stored in history.`})]})]})]})]})}Kt(B(Xr,{}),document.querySelector(`#app`));