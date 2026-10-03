import assert from "node:assert/strict";
import { mockIPC, clearMocks } from "@tauri-apps/api/mocks";
import { getIconMap } from "../src/platform/persistence/iconRepository.ts";
const data = "data:image/png;base64,AAAA";
const icon = (key: string) => ({source_key: key, keys: [key], data_url: data});
const seen: Array<string | null> = [];
const result = await getIconMap(async after => {
  seen.push(after);
  return after === null ? {entries: [{...icon("Cursor.exe"),keys:["Cursor.exe","cursor.exe"]}], next_after: "Cursor.exe"}
    : {entries:[icon("__proto__"),icon("应用")],next_after:null};
});
assert.deepEqual(seen,[null,"Cursor.exe"]);
assert.equal(result["cursor.exe"],data);assert.equal(result["__proto__"],data);
assert.equal(Object.getPrototypeOf(result),null);
for (const invalid of [
  {entries:[],next_after:"stuck"},
  {entries:[icon("z"),icon("a")],next_after:null},
  {entries:[icon("a"),icon("a")],next_after:null},
  {entries:[{...icon("a"),data_url:"file:///etc/passwd"}],next_after:null},
  {entries:[{...icon("a"),data_url:"data:image/png;base64,AAAA===="}],next_after:null},
  {entries:[{...icon("a"),keys:["a","a"]}],next_after:null},
  {entries:[icon("a")],next_after:"wrong"},
  {entries:[{...icon("a"),data_url:data+"A".repeat(512*1024)}],next_after:null},
]) await assert.rejects(getIconMap(async()=>invalid));
let calls=0;
await assert.rejects(getIconMap(async after=>{
  calls++; if(after)throw new Error("later page unavailable");
  return {entries:[icon("a")],next_after:"a"};
}),/later page unavailable/);
assert.equal(calls,2);
await assert.rejects(getIconMap(async()=>({entries:[icon("a")],next_after:"a"})),/Invalid cached icon entry/);
const descriptor=Object.getOwnPropertyDescriptor(globalThis,"window");
Object.defineProperty(globalThis,"window",{configurable:true,value:{}});
try {
  const commands: string[]=[];
  mockIPC((command,payload)=>{commands.push(command);assert.equal(payload.limit,64);assert.equal(payload.after,null);return {entries:[icon("app")],next_after:null};});
  assert.equal((await getIconMap()).app,data);assert.deepEqual(commands,["cmd_get_cached_icon_page"]);
  mockIPC(()=>{throw new Error("owner unavailable");});
  await assert.rejects(getIconMap(),/owner unavailable/);
}finally{clearMocks();if(descriptor)Object.defineProperty(globalThis,"window",descriptor);else Reflect.deleteProperty(globalThis,"window");}
console.log("Icon pagination, aliases, failure atomicity, prototype keys and owner-only transport passed");

const abort=new AbortController();
let cancelledCalls=0;
await assert.rejects(getIconMap(async()=>{cancelledCalls++;abort.abort();return {entries:[icon("a")],next_after:"a"};},abort.signal),/cancelled/);
assert.equal(cancelledCalls,1,"cancelled page must not schedule another request");

const unicode=await getIconMap(async()=>({entries:[icon("\ue000"),icon("😀")],next_after:null}));
assert.equal(unicode["😀"],data,"cursor order is SQLite UTF-8 binary, not JavaScript UTF-16 order");
let pages=0;
await assert.rejects(getIconMap(async()=>{
  const key=String(++pages).padStart(4,"0");return {entries:[icon(key)],next_after:key};
}),/pagination budget/);
assert.equal(pages,128);
