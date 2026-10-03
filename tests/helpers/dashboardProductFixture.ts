export function dashboardWireFixture(date = new Date(2026,3,18), duration = 120000) {
  const current = new Date(date);current.setHours(0,0,0,0);
  const previous = new Date(current);previous.setDate(previous.getDate()-1);
  const end = new Date(current);end.setDate(end.getDate()+1);
  return {sampled_at_ms:end.getTime(),configuration_revision:"a".repeat(64),
    tracking_health:{status:"unavailable",last_heartbeat_ms:null,live_cutoff_ms:0,stale_after_ms:8000},
    applications:[{app_key:"editor",app_name:"Editor",exe_name:"editor",category:"development",display_name_override:"Research"}],
    current:{start_ms:current.getTime(),end_ms:end.getTime(),active_ms:duration,apps:[{app_key:"editor",active_ms:duration}]},
    previous:{start_ms:previous.getTime(),end_ms:current.getTime(),active_ms:0,apps:[]},
    hours:Array.from({length:24},(_,hour)=>({hour,active_ms:hour===8?duration:0,categories:hour===8?[{category:"development",active_ms:duration}]:[]})),
  };
}
