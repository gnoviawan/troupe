var host = document.getElementById(p.hostId);
if (!host) return [];
var raw = host.getAttribute(p.attr);
if (!raw) return [];
host.removeAttribute(p.attr);
try {
  var parsed = JSON.parse(raw);
  return Array.isArray(parsed) ? parsed : [];
} catch (_) {
  return [];
}
