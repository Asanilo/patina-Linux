import assert from "node:assert/strict";
import { mockIPC, clearMocks } from "@tauri-apps/api/mocks";
import { deleteObservedAppSessions } from "../src/features/classification/services/classificationStore.ts";
const descriptor=Object.getOwnPropertyDescriptor(globalThis,"window");
Object.defineProperty(globalThis,"window",{configurable:true,value:{}});
const result={app_key:"steam.exe",matched_executables:2,deleted:{sessions_deleted:1,imported_exact_sessions_deleted:1,imported_time_buckets_deleted:0,import_batches_deleted:1}};
try {
  const calls: unknown[]=[];
  mockIPC((command,payload)=>{calls.push([command,payload]);assert.equal(command,"cmd_delete_canonical_app_history");return result;});
  assert.equal(await deleteObservedAppSessions(" SteamWebHelper.exe ","all"),2);
  assert.deepEqual(calls,[["cmd_delete_canonical_app_history",{request:{app_key:" SteamWebHelper.exe ",scope:"all",confirmed:true}}]]);
  // Alias expansion and today's calendar are owned by the backend, not this caller.
  calls.length=0;await deleteObservedAppSessions("Steam.exe","today");
  assert.deepEqual(calls,[["cmd_delete_canonical_app_history",{request:{app_key:"Steam.exe",scope:"today",confirmed:true}}]]);
  calls.length=0;assert.equal(await deleteObservedAppSessions("  "),0);assert.equal(calls.length,0);
  mockIPC(()=>{calls.push("failed");throw new Error("owner unavailable");});
  await assert.rejects(deleteObservedAppSessions("Steam.exe"),/owner unavailable/);assert.equal(calls.length,1,"must not retry or enumerate SQL");
  for(const invalid of [{...result,matched_executables:5000},{...result,deleted:{}},{...result,deleted:{...result.deleted,sessions_deleted:-1}}]){
    mockIPC(()=>invalid);await assert.rejects(deleteObservedAppSessions("Steam.exe"),/Invalid application cleanup/);
  }
} finally {clearMocks();if(descriptor)Object.defineProperty(globalThis,"window",descriptor);else Reflect.deleteProperty(globalThis,"window");}
console.log("Canonical cleanup delegates confirmed scope once, validates results and never falls back to SQL");
