// JavaScript policy implementation for payload formatting
var payload = context.getVariable("response.content");
if (payload) {
    var parsed = JSON.parse(payload);
    parsed.processed_at = new Date().toISOString();
    context.setVariable("response.content", JSON.stringify(parsed));
}
