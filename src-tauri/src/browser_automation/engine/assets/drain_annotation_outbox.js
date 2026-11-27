var host = BY_ID[9](document, p.hostId);
if (!host) return [];
var raw = attr(host, p.attr);
if (!raw) return [];
Element.prototype.removeAttribute.call(host, p.attr);
try {
  var parsed = JSON.parse(raw);
  return Array.isArray(parsed) ? parsed : [];
} catch (_) {
  return [];
}
