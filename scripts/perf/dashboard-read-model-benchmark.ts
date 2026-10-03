import { buildDashboardReadModel } from "../../src/features/dashboard/services/dashboardReadModel.ts";
import type { DashboardProductRead } from "../../src/platform/persistence/dashboardRepository.ts";
import { measureBenchmark, printBenchmarkReport } from "./benchmarkUtils.ts";

// Measure presentation of bounded backend quantities, not the retired session compiler.
const appCount = 2400;
const product: DashboardProductRead = {
  sampledAtMs: 1776506400000,
  configurationRevision: "a".repeat(64),
  trackingHealth: { status:"unavailable", lastHeartbeatMs:null, liveCutoffMs:0, staleAfterMs:8000 },
  applications: Array.from({length:appCount},(_,index)=>({appKey:`app-${index}`,exeName:`app-${index}`,appName:`Application ${index}`,category:"other",displayNameOverride:null})),
  previous: {date:"2026-04-17",duration:0,apps:[]},
  current: {date:"2026-04-18",duration:appCount*1000,apps:Array.from({length:appCount},(_,index)=>({appKey:`app-${index}`,duration:1000}))},
  hours: Array.from({length:24},(_,hour)=>({hour,duration:appCount/24*1000,categories:[{category:"other",duration:appCount/24*1000}]})),
};
const measurement = measureBenchmark("dashboard-product-presentation",400,25,()=>{buildDashboardReadModel(product);});
printBenchmarkReport({benchmark:"dashboard-product-presentation",measuredAt:new Date().toISOString(),measurements:[measurement],metadata:{appCount,hourCount:24}});
