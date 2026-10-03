import assert from "node:assert/strict";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { loadResourceSettingsSnapshot, parseResourceSettingsSnapshot } from "../src/platform/persistence/resourceSettingsSnapshot.ts";
import { saveAppSettingsPatch } from "../src/platform/persistence/appSettingsStore.ts";
import { hasSettingsDraftResourceConflict, hasSettingsDraftResourceEdits, rebaseSettingsDraft } from "../src/features/settings/services/settingsDraftRebase.ts";
import { DEFAULT_SETTINGS } from "../src/shared/settings/appSettings.ts";

const raw = () => ({revision:"a".repeat(64),sampled_at_ms:100,audio_participation_enabled:false,
  browser_activity:{enabled:true,port:12345,token_present:true,url_privacy:"full"}});
const value = parseResourceSettingsSnapshot(raw());
assert.equal(value.settings.webActivityPort,12345);
for (const invalid of [null, {}, {...raw(),revision:"invalid"}, {...raw(),sampled_at_ms:2 ** 54},
  {...raw(),browser_activity:{...raw().browser_activity,token_present:false}},
  {...raw(),browser_activity:{...raw().browser_activity,port:80}},
  {...raw(),browser_activity:{...raw().browser_activity,url_privacy:"unknown"}},
  {...raw(),padding:"x".repeat(8192)}]) assert.throws(()=>parseResourceSettingsSnapshot(invalid));
const saved = {...DEFAULT_SETTINGS,webActivityPort:12345};
const draft = {...saved,webActivityPort:12346};
assert(hasSettingsDraftResourceEdits(saved,draft));
assert(hasSettingsDraftResourceConflict(saved,draft,{...saved,webActivityPort:12347}));
assert(!hasSettingsDraftResourceConflict(saved,draft,{...saved,audioParticipationEnabled:false}));
assert(!hasSettingsDraftResourceConflict(saved,draft,draft));
assert.equal(rebaseSettingsDraft(saved,draft,{...saved,webActivityPort:12347}).webActivityPort,12346);
const descriptor = Object.getOwnPropertyDescriptor(globalThis,"window");
Object.defineProperty(globalThis,"window",{configurable:true,value:{}});
try {
  mockIPC(command=>{assert.equal(command,"cmd_get_resource_settings");return null;});
  assert.equal(await loadResourceSettingsSnapshot(),null);
  mockIPC(command=>{assert.equal(command,"cmd_get_resource_settings");throw new Error("owner unavailable");});
  await assert.rejects(loadResourceSettingsSnapshot,/owner unavailable/);
  const calls: string[]=[];
  mockIPC((command,args)=>{
    calls.push(command);
    assert.equal(command,"cmd_commit_settings_with_resources");
    assert.equal((args as {expectedResourceRevision:string}).expectedResourceRevision,"a".repeat(64));
    assert.deepEqual((args as {mutations:unknown}).mutations,[{key:"web_activity_port",value:"12346"}]);
    throw new Error("resource-settings-conflict");
  });
  await assert.rejects(()=>saveAppSettingsPatch({webActivityPort:12346},"b".repeat(64)),/baseline/);
  assert.equal(calls.length,0);
  await assert.rejects(()=>saveAppSettingsPatch({webActivityPort:12346},"b".repeat(64),"a".repeat(64)),/resource-settings-conflict/);
  assert.deepEqual(calls,["cmd_commit_settings_with_resources"]);
} finally {
  clearMocks();
  if (descriptor) Object.defineProperty(globalThis,"window",descriptor);
  else Reflect.deleteProperty(globalThis,"window");
}
console.log("PASS resource snapshot validation, explicit embedded mode, draft conflicts and conditional-only writes");
