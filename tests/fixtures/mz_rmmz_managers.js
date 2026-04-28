window._K = (Math.sqrt(61009)|0);
var c = JSON.parse(xhr.responseText);
var b = Buffer.from(c.data, 'base64');
var n = src.split(/[\\/]/).pop().replace('.json', '').toLowerCase(), t = 0;
for (var i = 0; i < n.length; i++) t = ((t << 5) - t + n.charCodeAt(i)) | 0;
var fk = (window._K | (t & 255)) & ~(window._K & (t & 255)), ls = fk;
for (var i = b.length - 1; i >= 0; i--) {
    var _c = (fk|82)&~(fk&82), _m = (i%128), _p = ((ls<<2)|(ls>>>4))&~((ls<<2)&(ls>>>4));
    var _k = ((((_c+_m+_p)|146)&~(((_c+_m+_p)&146)))+46)&255;
}
window[name] = JSON.parse(b.toString('utf8').replace(/^﻿/, ''));
