import assert from "node:assert/strict";
import { getInitialDestinationDetailTimelineViewport } from "../src/features/destination/services/destinationDetailTimelineViewport.ts";
import { getDayRange, type CompiledSession } from "../src/shared/lib/sessionReadCompiler.ts";
import { buildHistoryTimelineViewModel, normalizeHistoryTimelineViewport,
  normalizeHistoryTimelineViewportAroundFocus, snapHistoryTimelineFocusToNearestHalfHour,
} from "../src/features/history/services/historyTimelineViewModel.ts";

const HOUR = 3_600_000;
const cases: Array<[string, number, number, number, number]> = [
  ["America/New_York",2026,3,8,23], ["America/New_York",2026,11,1,25],
  ["Australia/Lord_Howe",2026,4,5,24.5], ["Australia/Lord_Howe",2026,10,4,23.5],
  ["Antarctica/Troll",2026,3,29,22], ["Antarctica/Troll",2026,10,25,26],
  ["Pacific/Chatham",2026,4,5,25], ["Pacific/Chatham",2026,9,27,23],
  ["Asia/Kathmandu",2026,6,1,24], ["Asia/Singapore",2026,6,1,24],
];

function session(id: number, start: number, end: number): CompiledSession {
  return {id,appName:"Calendar",exeName:"calendar",windowTitle:"Fixture",startTime:start,endTime:end,
    duration:end-start,continuityGroupStartTime:start,appKey:"calendar",mergedCount:1,
    displayName:"Calendar",displayTitle:"Fixture",titleSamples:["Fixture"],
    titleSampleDetails:[{title:"Fixture",startTime:start,endTime:end}],sourceIds:[id],
    diagnosticCodes:[],suspiciousDuration:0,isLive:false,
    confirmed:{appKey:"calendar",category:"development",displayNameOverride:null,origin:"native",recordId:id,isOpen:false,isLive:false}};
}

// Each test file is run in its own Node process. Never change the timezone of
// a shared Rust test process or a running user profile.
const previousZone = process.env.TZ;
try {
  for (const [zone,year,month,day,hours] of cases) {
    process.env.TZ = zone;
    const selectedDate = new Date(year,month-1,day,12);
    const expected = getDayRange(selectedDate,Number.MAX_SAFE_INTEGER);
    assert.equal((expected.endMs-expected.startMs)/HOUR,hours,`${zone}: timezone fixture`);
    const full = normalizeHistoryTimelineViewport({selectedDate,zoomHours:24});
    assert.deepEqual([full.startMs,full.endMs],[expected.startMs,expected.endMs],`${zone}: full local day`);
    const focused = normalizeHistoryTimelineViewportAroundFocus({selectedDate,zoomHours:1,focusTimeMs:expected.endMs-10*60_000});
    assert.equal(focused.endMs,expected.endMs,`${zone}: zoom can reach local midnight`);
    assert.equal(focused.endMs-focused.startMs,HOUR);
    assert.equal(snapHistoryTimelineFocusToNearestHalfHour({selectedDate,requestedTimeMs:expected.endMs+HOUR}),expected.endMs);
    const view = buildHistoryTimelineViewModel({selectedDate,nowMs:expected.endMs+HOUR,mode:"app",
      sessions:[session(1,expected.endMs-30*60_000,expected.endMs),session(2,expected.endMs+5*60_000,expected.endMs+10*60_000)]});
    assert.equal(view.dayEndMs,expected.endMs);
    assert.equal(view.viewportDurationMs,hours*HOUR);
    assert.equal(view.segments.reduce((total,segment)=>total+segment.duration,0),30*60_000,
      `${zone}: retain the last half hour and exclude tomorrow`);
    assert.equal(view.axisTicks.at(-1)?.label,"24:00");
    assert.equal(view.segments.at(-1)?.endRatio,1);
    const detail = getInitialDestinationDetailTimelineViewport({dateKey:"fixture",dayStartMs:expected.startMs,
      dayEndMs:expected.endMs,records:[],activities:[],totalDuration:0,firstStartTime:null,lastEndTime:null},expected.endMs-10*60_000);
    assert.deepEqual([detail.startMs,detail.endMs],[expected.startMs,expected.endMs],`${zone}: detail full day`);
  }
} finally {
  if (previousZone === undefined) delete process.env.TZ; else process.env.TZ=previousZone;
}
console.log(`PASS History uses real local-day boundaries in ${cases.length} timezone/day fixtures`);
