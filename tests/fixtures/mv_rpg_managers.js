window._K=(Math.sqrt(23104)|0);
var c=JSON.parse(xhr.responseText);var b=Buffer.from(c.data,'base64');
var n=src.split(/[\\/]/).pop().replace('.json', ''),t=0;
for(var i=0;i<n.length;i++)t=((t<<5)-t+n.charCodeAt(i))|0;
var fk=(window._K|(t&255))&~(window._K&(t&255)),ls=fk;
for(var i=b.length-1;i>=0;i++){
  var _c=(fk|85)&~(fk&85),_m=(i%128),_p=((ls<<2)|(ls>>>4))&~((ls<<2)&(ls>>>4));
  var _k=((((_c+_m+_p)|180)&~(((_c+_m+_p)&180)))+36)&255;
}
window[name]=JSON.parse(b.toString('utf8').replace(/^﻿/, ''));
