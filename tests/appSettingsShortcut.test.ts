import assert from "node:assert/strict";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { saveMinSessionSecsSetting, loadLatestTrackingPauseSetting } from "../src/app/services/appSettingsRuntimeService.ts";
import { saveAppSettingsPatch } from "../src/platform/persistence/appSettingsStore.ts";

const originalWindow=Object.getOwnPropertyDescriptor(globalThis,"window");
Object.defineProperty(globalThis,"window",{configurable:true,value:{}});
const revision="a".repeat(64);
const snapshot={revision:"b".repeat(64),sampled_at_ms:100,last_heartbeat_ms:null,last_successful_sample_ms:null,
  settings:{idle_timeout_secs:900,timeline_merge_gap_secs:180,min_session_secs:360,tracking_paused:true,
    audio_participation_enabled:true,web_activity_enabled:false,web_activity_port:12345,
    web_activity_token_present:false,web_activity_url_privacy:"full"}};
try {
  const calls:string[]=[];
  mockIPC((command,args)=>{
    calls.push(command);
    assert.equal(command,"cmd_commit_settings_if_revision");
    assert.deepEqual(args,{mutations:[{key:"min_session_secs",value:"360"}],expectedRevision:revision});
    return snapshot;
  });
  await assert.rejects(saveMinSessionSecsSetting(360,""),/baseline/);
  await assert.rejects(saveAppSettingsPatch({minSessionSecs:360}),/baseline/);
  await assert.rejects(saveAppSettingsPatch({trackingPaused:true}),/baseline/);
  assert.equal(calls.length,0);
  const confirmed=await saveMinSessionSecsSetting(360,revision);
  assert.equal(confirmed.revision,snapshot.revision);
  assert.equal(confirmed.settings.minSessionSecs,360);
  assert.deepEqual(calls,["cmd_commit_settings_if_revision"]);
  calls.length=0;
  mockIPC(command=>{calls.push(command);throw new Error("product-settings-conflict");});
  await assert.rejects(saveMinSessionSecsSetting(360,revision),/product-settings-conflict/);
  assert.deepEqual(calls,["cmd_commit_settings_if_revision"]);
  mockIPC(command=>{assert.equal(command,"cmd_get_product_settings");return snapshot;});
  assert.equal(await loadLatestTrackingPauseSetting(),true);
} finally {
  clearMocks();
  if (originalWindow) Object.defineProperty(globalThis,"window",originalWindow);
  else Reflect.deleteProperty(globalThis,"window");
}
console.log("PASS shortcut uses the original policy revision, never pre-reads or falls back, and pause reads stay scoped");
