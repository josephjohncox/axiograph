import { ReadOnlyClient, UNSUPPORTED } from "./read-only-client";
import { element } from "../render/dom";
interface ServerContext {
  serverControlsEl?: HTMLElement | null;
  setQueryClient(client: ReadOnlyClient | null): void;
  setAxqlStatus(text: string): void;
}
export async function initServerControls(ctx: ServerContext) {
  ctx.setQueryClient(null);
  const target = ctx.serverControlsEl;
  if (!target) return;
  target.textContent = `Offline/local graph inspection. ${UNSUPPORTED}`;
  if (!["http:", "https:"].includes(window.location.protocol)) return;
  try {
    const client = new ReadOnlyClient();
    const { capabilities, status } = await client.discover();
    // Build everything before committing visible state or enabling queries.
    const statusView = element("pre", {}, JSON.stringify(status, null, 2));
    const schema = element("details", {}, element("summary", {}, "Descriptive QueryIrV1 input profile (Rust validation is authoritative)"), element("pre", {}, JSON.stringify(capabilities.query_schema, null, 2)));
    target.replaceChildren(
      element("div", {}, "Connected read-only database. Receipt metadata below is opaque to this client: server image authentication is not HTTP authentication, graph identity binding, or a verified query certificate."),
      statusView, schema, element("div", {}, UNSUPPORTED),
    );
    ctx.setQueryClient(client);
    ctx.setAxqlStatus("Ready: QueryIrV1 JSON. Server-image-local IDs; offline graph/context selection is not sent. Highlighting unavailable without an image binding.");
  } catch (error) {
    ctx.setQueryClient(null);
    target.textContent = `Read-only API unavailable: ${String(error)}. Local graph/draft inspection remains available.`;
    ctx.setAxqlStatus(`Query unavailable: ${String(error)}`);
  }
}
