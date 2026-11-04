// A mock MCP client over stdio: the shape Claude Desktop uses.
//
// It spawns `rusty-brick-chess mcp`, writes one JSON-RPC message per line and
// reads the answers. What it proves is what a client would notice: initialize
// answers, the tools list is non-empty, a read runs, and a write asks for
// approval instead of performing it.
const { spawn } = require("child_process");
const readline = require("readline");

const BIN = process.env.RBC_BIN;
const DB = process.env.RBC_DB;

function client() {
  const child = spawn(BIN, ["mcp"], {
    env: { ...process.env, RBC_DB: DB },
    stdio: ["pipe", "pipe", "inherit"],
  });
  const lines = [];
  const waiting = [];
  readline.createInterface({ input: child.stdout }).on("line", (line) => {
    let parsed;
    try {
      parsed = JSON.parse(line);
    } catch {
      return;
    }
    lines.push(parsed);
    const next = waiting.shift();
    if (next) next(parsed);
  });
  return {
    child,
    send(message) {
      child.stdin.write(JSON.stringify(message) + "\n");
    },
    // The answer to one id, waiting if it has not arrived.
    async answer(id) {
      const seen = lines.find((line) => line.id === id);
      if (seen) return seen;
      return new Promise((resolve, reject) => {
        const timer = setTimeout(() => reject(new Error(`no answer to ${id}`)), 8000);
        waiting.push((line) => {
          clearTimeout(timer);
          if (line.id === id) resolve(line);
          else reject(new Error(`expected id ${id}, got ${line.id}`));
        });
      });
    },
    close() {
      child.stdin.end();
      child.kill();
    },
  };
}

(async () => {
  const mcp = client();
  const out = {};

  mcp.send({ jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-06-18" } });
  const init = await mcp.answer(1);
  out.protocol = init.result?.protocolVersion;
  out.server = init.result?.serverInfo?.name;
  // A client signals it is ready; a notification is never answered.
  mcp.send({ jsonrpc: "2.0", method: "notifications/initialized" });

  mcp.send({ jsonrpc: "2.0", id: 2, method: "tools/list" });
  const tools = await mcp.answer(2);
  out.tools = tools.result?.tools?.length ?? 0;

  mcp.send({ jsonrpc: "2.0", id: 3, method: "resources/list" });
  const resources = await mcp.answer(3);
  out.resources = resources.result?.resources?.length ?? 0;

  // A read runs against the shared database.
  mcp.send({
    jsonrpc: "2.0", id: 4, method: "tools/call",
    params: { name: "list_workspaces", arguments: {} },
  });
  const read = await mcp.answer(4);
  out.read = read.result?.structuredContent?.code;
  out.readOk = read.result?.isError === false;

  // A write must ask for approval: the default profile grants MCP nothing.
  mcp.send({
    jsonrpc: "2.0", id: 5, method: "tools/call",
    params: { name: "create_board", arguments: { workspace_id: "w", title: "t", fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1" } },
  });
  const write = await mcp.answer(5);
  const structured = write.result?.structuredContent ?? {};
  out.writeCode = structured.code;
  out.writeStatus = structured.data?.status;
  out.approvalId = structured.data?.approval_id;
  out.polls = structured.data?.poll;

  // And polling it says pending.
  if (out.approvalId) {
    mcp.send({
      jsonrpc: "2.0", id: 6, method: "tools/call",
      params: { name: "approval_status", arguments: { approval_id: out.approvalId } },
    });
    const poll = await mcp.answer(6);
    out.pollStatus = poll.result?.structuredContent?.data?.status;
  }

  mcp.close();
  console.log(JSON.stringify(out));
  const ok =
    out.protocol === "2025-06-18" &&
    out.server === "rusty-brick-chess" &&
    out.tools > 10 &&
    out.resources >= 1 &&
    out.readOk === true &&
    out.writeCode === "approval_required" &&
    out.writeStatus === "approval_required" &&
    Boolean(out.approvalId) &&
    out.polls === "approval_status" &&
    out.pollStatus === "pending";
  console.log(ok ? "MCP_STDIO_PASS" : "MCP_STDIO_FAIL");
  process.exit(ok ? 0 : 1);
})().catch((error) => {
  console.error(error);
  process.exit(1);
});
