// A mock MCP client over HTTP: the shape a local agent uses.
//
// Refusals first (no token, wrong token, a traversal uri), then the happy path,
// then rotation invalidating the old token. A check that only exercises the
// happy path would pass a build with no auth at all.
const BASE = process.env.RBC_URL || "http://127.0.0.1:8090";

async function call(token, body) {
  const response = await fetch(`${BASE}/mcp`, {
    method: "POST",
    headers: {
      "content-type": "application/json",
      ...(token ? { authorization: `Bearer ${token}` } : {}),
    },
    body: JSON.stringify(body),
  });
  const text = await response.text();
  let json = null;
  try {
    json = JSON.parse(text);
  } catch {
    // A refusal has no body worth parsing.
  }
  return { status: response.status, json };
}

(async () => {
  const config = await (await fetch(`${BASE}/api/mcp/config`)).json();
  const token = config.token;
  const out = { protocol: config.protocol };

  out.noToken = (await call(null, { jsonrpc: "2.0", id: 1, method: "ping" })).status;
  out.wrongToken = (await call("mcp-not-the-token", { jsonrpc: "2.0", id: 2, method: "ping" })).status;

  out.initialize = (await call(token, { jsonrpc: "2.0", id: 3, method: "initialize", params: {} })).json?.result?.protocolVersion;

  const tools = await call(token, { jsonrpc: "2.0", id: 4, method: "tools/list" });
  out.tools = tools.json?.result?.tools?.length ?? 0;

  const resources = await call(token, { jsonrpc: "2.0", id: 5, method: "resources/list" });
  const uris = (resources.json?.result?.resources ?? []).map((r) => r.uri);
  out.resources = uris.length;

  // A real resource reads.
  const workspaces = uris.find((uri) => uri === "rbc://workspaces");
  if (workspaces) {
    const read = await call(token, {
      jsonrpc: "2.0", id: 6, method: "resources/read", params: { uri: workspaces },
    });
    out.resourceRead = read.json?.result?.contents?.[0]?.mimeType;
  }

  // A traversal attempt resolves to nothing.
  const traversal = await call(token, {
    jsonrpc: "2.0", id: 7, method: "resources/read",
    params: { uri: "rbc://board/../../etc/passwd" },
  });
  out.traversal = traversal.json?.error?.code ?? "no error";

  // A read tool runs; a write asks for approval and runs nothing.
  const read = await call(token, {
    jsonrpc: "2.0", id: 8, method: "tools/call",
    params: { name: "list_workspaces", arguments: {} },
  });
  out.readOk = read.json?.result?.isError === false;

  const write = await call(token, {
    jsonrpc: "2.0", id: 9, method: "tools/call",
    params: { name: "create_board", arguments: { workspace_id: "w", title: "t", fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1" } },
  });
  const structured = write.json?.result?.structuredContent ?? {};
  out.writeCode = structured.code;
  out.approvalId = structured.data?.approval_id;

  // Rotation: the old token stops working, the new one starts.
  const rotated = await (await fetch(`${BASE}/api/mcp/rotate`, { method: "POST" })).json();
  out.oldAfterRotate = (await call(token, { jsonrpc: "2.0", id: 10, method: "ping" })).status;
  out.newAfterRotate = (await call(rotated.token, { jsonrpc: "2.0", id: 11, method: "ping" })).status;

  console.log(JSON.stringify(out));
  const ok =
    out.noToken === 401 &&
    out.wrongToken === 401 &&
    out.initialize === "2025-06-18" &&
    out.tools > 10 &&
    out.resources >= 1 &&
    out.resourceRead === "application/json" &&
    out.traversal === -32602 &&
    out.readOk === true &&
    out.writeCode === "approval_required" &&
    Boolean(out.approvalId) &&
    out.oldAfterRotate === 401 &&
    out.newAfterRotate === 200;
  console.log(ok ? "MCP_HTTP_PASS" : "MCP_HTTP_FAIL");
  process.exit(ok ? 0 : 1);
})().catch((error) => {
  console.error(error);
  process.exit(1);
});
