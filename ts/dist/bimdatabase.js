!// modules are defined as an array
// [ module function, map of requires ]
//
// map of requires is short require name -> numeric require
//
// anything defined in a previous bundle is accessed via the
// orig method which is the require for previous bundles
function(e,t,n,r,o){/* eslint-disable no-undef */var l="undefined"!=typeof globalThis?globalThis:"undefined"!=typeof self?self:"undefined"!=typeof window?window:"undefined"!=typeof global?global:{},u="function"==typeof l[r]&&l[r],i=u.cache||{},d="undefined"!=typeof module&&"function"==typeof module.require&&module.require.bind(module);function a(t,n){if(!i[t]){if(!e[t]){// if we cannot find the module within our internal map or
// cache jump to the current global require ie. the last bundle
// that was added to the page.
var o="function"==typeof l[r]&&l[r];if(!n&&o)return o(t,!0);// If there are other bundles on this page the require from the
// previous one is saved to 'previousRequire'. Repeat this as
// many times as there are bundles until the module is found or
// we exhaust the require chain.
if(u)return u(t,!0);// Try the node require function if it exists.
if(d&&"string"==typeof t)return d(t);var f=Error("Cannot find module '"+t+"'");throw f.code="MODULE_NOT_FOUND",f}s.resolve=function(n){var r=e[t][1][n];return null!=r?r:n},s.cache={};var c=i[t]=new a.Module(t);e[t][0].call(c.exports,s,c,c.exports,this)}return i[t].exports;function s(e){var t=s.resolve(e);return!1===t?{}:a(t)}}a.isParcelRequire=!0,a.Module=function(e){this.id=e,this.bundle=a,this.exports={}},a.modules=e,a.cache=i,a.parent=u,a.register=function(t,n){e[t]=[function(e,t){t.exports=n},{}]},Object.defineProperty(a,"root",{get:function(){return l[r]}}),l[r]=a;for(var f=0;f<t.length;f++)a(t[f]);if(n){// Expose entry point to Node, AMD or browser globals
// Based on https://github.com/ForbesLindesay/umd/blob/master/template.js
var c=a(n);// CommonJS
"object"==typeof exports&&"undefined"!=typeof module?module.exports=c:"function"==typeof define&&define.amd?define(function(){return c}):o&&(this[o]=c)}}({bvhvx:[function(e,t,n){var r,o=e("@parcel/transformer-js/src/esmodule-helpers.js");o.defineInteropFlag(n),o.export(n,"BimDatabase",()=>r),function(e){function t(e,t){let n=document.createElement("div");n.classList.add("other-data-entry"),e.insertBefore(n,t);let r=document.createElement("input");r.type="text",r.classList.add("key"),n.appendChild(r);let o=document.createElement("input");o.type="text",o.classList.add("value"),n.appendChild(o);let l=document.createElement("input");return l.type="button",l.value="−",l.addEventListener("click",()=>n.parentNode?.removeChild(n)),n.appendChild(l),[r,o]}function n(){let e=document.getElementById("bimdb-ae-other-data");if(null===e)return;let n=e.parentElement;if(null===n)return;let r=e.form;if(null===r)return;r.addEventListener("submit",t=>(function(e,t,n,r){r.preventDefault();// reassemble text area value
    let o=Array.prototype.slice.call(t.querySelectorAll("div.other-data-entry"),0),l={};for(let e of o){let t=e.querySelector("input.key");if(null===t)continue;let n=e.querySelector("input.value");null!==n&&(l[t.value]=n.value)}// remove custom form fields
    for(let e of(n.value=JSON.stringify(l),o))e.parentNode?.removeChild(e);// submit modified form
    e.submit()})(r,n,e,t));// add new-entry piece
let o=document.createElement("div");o.classList.add("add-other-data-entry"),n.appendChild(o);// disassemble text area
let l=JSON.parse(e.value),u=Object.keys(l);for(let e of u){let r=l[e],[u,i]=t(n,o);u.value=e,i.value=r}let i=document.createElement("input");i.type="button",i.value="+",i.addEventListener("click",()=>t(n,o)),o.appendChild(i),e.style.display="none"}e.setUpAddEdit=function(){document.addEventListener("DOMContentLoaded",n)}}(r||(r={})),window.BimDatabase=r},{"@parcel/transformer-js/src/esmodule-helpers.js":"WSxqz"}],WSxqz:[function(e,t,n){n.interopDefault=function(e){return e&&e.__esModule?e:{default:e}},n.defineInteropFlag=function(e){Object.defineProperty(e,"__esModule",{value:!0})},n.exportAll=function(e,t){return Object.keys(e).forEach(function(n){"default"===n||"__esModule"===n||t.hasOwnProperty(n)||Object.defineProperty(t,n,{enumerable:!0,get:function(){return e[n]}})}),t},n.export=function(e,t,n){Object.defineProperty(e,t,{enumerable:!0,get:n})}},{}]},["bvhvx"],"bvhvx","parcelRequire4688")//# sourceMappingURL=bimdatabase.js.map
;
//# sourceMappingURL=bimdatabase.js.map
