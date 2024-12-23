!function(e,t,n,l,r){var o="undefined"!=typeof globalThis?globalThis:"undefined"!=typeof self?self:"undefined"!=typeof window?window:"undefined"!=typeof global?global:{},d="function"==typeof o[l]&&o[l],i=d.cache||{},u="undefined"!=typeof module&&"function"==typeof module.require&&module.require.bind(module);function a(t,n){if(!i[t]){if(!e[t]){var r="function"==typeof o[l]&&o[l];if(!n&&r)return r(t,!0);if(d)return d(t,!0);if(u&&"string"==typeof t)return u(t);var c=Error("Cannot find module '"+t+"'");throw c.code="MODULE_NOT_FOUND",c}s.resolve=function(n){var l=e[t][1][n];return null!=l?l:n},s.cache={};var p=i[t]=new a.Module(t);e[t][0].call(p.exports,s,p,p.exports,o)}return i[t].exports;function s(e){var t=s.resolve(e);return!1===t?{}:a(t)}}a.isParcelRequire=!0,a.Module=function(e){this.id=e,this.bundle=a,this.exports={}},a.modules=e,a.cache=i,a.parent=d,a.register=function(t,n){e[t]=[function(e,t){t.exports=n},{}]},Object.defineProperty(a,"root",{get:function(){return o[l]}}),o[l]=a;for(var c=0;c<t.length;c++)a(t[c]);if(n){var p=a(n);"object"==typeof exports&&"undefined"!=typeof module?module.exports=p:"function"==typeof define&&define.amd&&define(function(){return p})}}({bvhvx:[function(e,t,n,l){var r,o,d=e("@parcel/transformer-js/src/esmodule-helpers.js");d.defineInteropFlag(n),d.export(n,"BimDatabase",()=>o);var i=e("./add_edit"),u=e("./coupling_add_edit");(r=o||(o={})).setUpAddEdit=function(){document.addEventListener("DOMContentLoaded",i.AddEdit.doSetUp)},r.setUpCouplingAddEdit=function(){document.addEventListener("DOMContentLoaded",u.CouplingAddEdit.doSetUp)},window.BimDatabase=o},{"./add_edit":"k4LrC","./coupling_add_edit":"821xA","@parcel/transformer-js/src/esmodule-helpers.js":"78wFE"}],k4LrC:[function(e,t,n,l){var r,o=e("@parcel/transformer-js/src/esmodule-helpers.js");o.defineInteropFlag(n),o.export(n,"AddEdit",()=>r),function(e){function t(e,t){let n=document.createElement("div");n.classList.add("other-data-entry"),e.insertBefore(n,t);let l=document.createElement("input");l.type="text",l.classList.add("key"),n.appendChild(l);let r=document.createElement("input");r.type="text",r.classList.add("value"),n.appendChild(r);let o=document.createElement("input");return o.type="button",o.value="−",o.addEventListener("click",()=>n.parentNode?.removeChild(n)),n.appendChild(o),[l,r]}e.doSetUp=function(){let e=document.getElementById("bimdb-ae-other-data");if(null===e)return;let n=e.parentElement;if(null===n)return;let l=e.form;if(null===l)return;l.addEventListener("submit",t=>(function(e,t,n,l){l.preventDefault();let r=Array.prototype.slice.call(t.querySelectorAll("div.other-data-entry"),0),o={};for(let e of r){let t=e.querySelector("input.key");if(null===t)continue;let n=e.querySelector("input.value");null!==n&&(o[t.value]=n.value)}for(let e of(n.value=JSON.stringify(o),r))e.parentNode?.removeChild(e);e.submit()})(l,n,e,t));let r=document.createElement("div");r.classList.add("add-other-data-entry"),n.appendChild(r);let o=JSON.parse(e.value);for(let e of Object.keys(o)){let l=o[e],[d,i]=t(n,r);d.value=e,i.value=l}let d=document.createElement("input");d.type="button",d.value="+",d.addEventListener("click",()=>{let[e,l]=t(n,r);e.focus()}),r.appendChild(d),e.style.display="none";let i=document.getElementById("bimdb-ae-company");null!==i&&i.focus()}}(r||(r={}))},{"@parcel/transformer-js/src/esmodule-helpers.js":"78wFE"}],"78wFE":[function(e,t,n,l){n.interopDefault=function(e){return e&&e.__esModule?e:{default:e}},n.defineInteropFlag=function(e){Object.defineProperty(e,"__esModule",{value:!0})},n.exportAll=function(e,t){return Object.keys(e).forEach(function(n){"default"===n||"__esModule"===n||Object.prototype.hasOwnProperty.call(t,n)||Object.defineProperty(t,n,{enumerable:!0,get:function(){return e[n]}})}),t},n.export=function(e,t,n){Object.defineProperty(e,t,{enumerable:!0,get:n})}},{}],"821xA":[function(e,t,n,l){var r,o=e("@parcel/transformer-js/src/esmodule-helpers.js");o.defineInteropFlag(n),o.export(n,"CouplingAddEdit",()=>r),function(e){function t(e){let t=e.querySelectorAll(".vehicle-entry");for(let e=0;e<t.length;e++){let n=t.item(e),l=n.querySelector(".up-button");null!==l&&(l.disabled=0===e);let r=n.querySelector(".down-button");null!==r&&(r.disabled=e===t.length-1)}}function n(e,n){let l=document.createElement("div");l.classList.add("vehicle-entry"),e.insertBefore(l,n);let r=document.createElement("input");r.type="text",r.classList.add("vehicle-number"),l.appendChild(r);let o=document.createElement("input");o.type="button",o.value="−",o.addEventListener("click",()=>{l.parentNode?.removeChild(l),t(e)}),l.appendChild(o);let d=document.createElement("input");d.type="button",d.classList.add("up-button"),d.value="↑",d.addEventListener("click",()=>{l.parentNode?.insertBefore(l,l.previousElementSibling),t(e)}),l.appendChild(d);let i=document.createElement("input");return i.type="button",i.classList.add("down-button"),i.value="↓",i.addEventListener("click",()=>(function(e,n){let l=n.nextElementSibling,r=null!==l?l.nextElementSibling:null;n.parentNode?.insertBefore(n,r),t(e)})(e,l)),l.appendChild(i),t(e),r}e.doSetUp=function(){let e=document.getElementById("bimdb-cae-vehicles");if(null===e)return;let t=e.parentElement;if(null===t)return;let l=e.form;if(null===l)return;l.addEventListener("submit",n=>(function(e,t,n,l){l.preventDefault();let r=Array.prototype.slice.call(t.querySelectorAll(".vehicle-entry"),0),o=[];for(let e of r){let t=e.querySelector("input.vehicle-number");null!==t&&o.push(t.value)}for(let e of(n.value=o.join("\n"),r))e.parentNode?.removeChild(e);e.submit()})(l,t,e,n));let r=document.createElement("div");for(let l of(r.classList.add("add-vehicle"),t.appendChild(r),e.value.split("\n").map(e=>e.trim()).filter(e=>e.length>0)))n(t,r).value=l;let o=document.createElement("input");o.type="button",o.value="+",o.addEventListener("click",()=>{n(t,r).focus()}),r.appendChild(o),e.style.display="none";let d=document.getElementById("bimdb-cae-company");null!==d&&d.focus()}}(r||(r={}))},{"@parcel/transformer-js/src/esmodule-helpers.js":"78wFE"}]},["bvhvx"],"bvhvx","parcelRequire94c2");
//# sourceMappingURL=bimdatabase.js.map
