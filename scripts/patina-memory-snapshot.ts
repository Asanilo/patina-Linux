import { open, readFile, readlink, readdir } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

type Role = "desktop" | "daemon";
export interface ProcessInfo {
  pid: number;
  ppid: number;
  name: string;
  role: Role | null;
  startTime: string;
}

export function parseMemory(text: string) {
  const fields = new Map<string, number>();
  for (const line of text.split("\n")) {
    const match = /^(\w+):\s+(\d+)\s+kB$/.exec(line.trim());
    if (match) fields.set(match[1], Number(match[2]));
  }
  const privateFields = ["Private_Clean", "Private_Dirty", "Private_Hugetlb"];
  return {
    rssKiB: fields.get("Rss") ?? null,
    pssKiB: fields.get("Pss") ?? null,
    ussKiB: privateFields.every((key) => fields.has(key))
      ? privateFields.reduce((sum, key) => sum + fields.get(key)!, 0) : null,
    swapKiB: fields.get("Swap") ?? null,
  };
}

export function processRole(executable: string): Role | null {
  const clean = executable.replace(/ \(deleted\)$/, "");
  return clean === "/usr/bin/Patina" ? "desktop" : clean === "/usr/bin/patinad" ? "daemon" : null;
}

export function classifyProcesses(processes: ProcessInfo[]) {
  const byPid = new Map(processes.map((item) => [item.pid, item]));
  return processes.flatMap((item) => {
    const visited = new Set<number>();
    let current: ProcessInfo | undefined = item;
    while (current && !visited.has(current.pid)) {
      visited.add(current.pid);
      if (current.role) return [{ ...item, owner: current.role, rootPid: current.pid }];
      current = byPid.get(current.ppid);
    }
    return [];
  });
}

function startTime(stat: string): string {
  const value = stat.slice(stat.lastIndexOf(")") + 2).split(/\s+/)[19];
  if (!value || !/^\d+$/.test(value)) throw new Error("invalid proc stat");
  return value;
}

export function summarizeMemory(rows: Array<{ owner: Role } & ReturnType<typeof parseMemory>>) {
  return (["desktop", "daemon"] as const).map((owner) => {
    const group = rows.filter((row) => row.owner === owner);
    const sum = (key: keyof ReturnType<typeof parseMemory>) =>
      group.every((row) => row[key] !== null) ? group.reduce((total, row) => total + row[key]!, 0) : null;
    return { owner, processes: group.length, rssKiB: sum("rssKiB"), pssKiB: sum("pssKiB"),
      ussKiB: sum("ussKiB"), swapKiB: sum("swapKiB"), missingPssProcesses: group.filter((row) => row.pssKiB === null).length };
  });
}

export async function captureMemory(label: string) {
  if (process.platform !== "linux") throw new Error("memory snapshots require Linux /proc");
  const startedAt = new Date().toISOString();
  const processes: ProcessInfo[] = [];
  let unreadableProcesses = 0;
  for (const pidText of await readdir("/proc")) {
    if (!/^\d+$/.test(pidText)) continue;
    try {
      const root = `/proc/${pidText}`;
      const status = await readFile(`${root}/status`, "utf8");
      if (Number(/^Uid:\s+(\d+)/m.exec(status)?.[1]) !== process.getuid!()) continue;
      const stat = startTime(await readFile(`${root}/stat`, "utf8"));
      const executable = await readlink(`${root}/exe`).catch(() => "");
      processes.push({ pid: Number(pidText), ppid: Number(/^PPid:\s+(\d+)/m.exec(status)?.[1]),
        name: /^Name:\s+(.+)$/m.exec(status)?.[1] ?? "unknown", role: processRole(executable), startTime: stat });
    } catch { unreadableProcesses += 1; }
  }
  const rows = [];
  for (const item of classifyProcesses(processes)) {
    let memory = parseMemory("");
    let error: string | null = null;
    try {
      const root = `/proc/${item.pid}`;
      memory = parseMemory(await readFile(`${root}/smaps_rollup`, "utf8"));
      if (startTime(await readFile(`${root}/stat`, "utf8")) !== item.startTime) throw new Error("pid-reused");
    } catch {
      memory = parseMemory("");
      error = "process-exited-reused-or-unreadable";
    }
    rows.push({ ...item, ...memory, error });
  }
  return { format: "patina.memory-snapshot.v1", label, startedAt, finishedAt: new Date().toISOString(),
    scope: "current-user installed /usr/bin/Patina and /usr/bin/patinad plus live descendants; non-atomic sample",
    unreadableProcesses, processes: rows, totals: summarizeMemory(rows) };
}

export async function writeMemoryEvidence(output: string, value: unknown) {
  const file = await open(output, "wx", 0o600);
  try { await file.writeFile(JSON.stringify(value, null, 2) + "\n", "utf8"); await file.sync(); }
  finally { await file.close(); }
}

async function main() {
  const args = process.argv.slice(2);
  let label = "observed";
  let output: string | undefined;
  for (let i = 0; i < args.length; i += 2) {
    if (!args[i + 1] || !["--label", "--output"].includes(args[i])) throw new Error("Usage: patina-memory-snapshot [--label state] [--output new-file.json]");
    if (args[i] === "--label") label = args[i + 1];
    else output = args[i + 1];
  }
  const evidence = await captureMemory(label);
  if (output) await writeMemoryEvidence(path.resolve(output), evidence);
  process.stdout.write(JSON.stringify(evidence, null, 2) + "\n");
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => { console.error(String(error)); process.exitCode = 1; });
}
