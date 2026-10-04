import { invoke } from "@tauri-apps/api/core";
import { getExactHistorySnapshot, type ExactHistoryRead } from "./historyRepository.ts";
import { parseActivityHours, type ActivityHourRead } from "./hourlyActivitySnapshot.ts";
import type { AppCategory } from "../../shared/classification/categoryTokens.ts";
import { getUiTextLanguage, type UiLanguage } from "../../shared/copy/uiText.ts";

export interface HistoryProductRead extends ExactHistoryRead {hours: ActivityHourRead[]}

export async function getHistoryProductSnapshot(fromMs:number,toMs:number,
  request:(from:number,to:number)=>Promise<unknown> = (from,to)=>invoke("cmd_get_history_product",{fromMs:from,toMs:to,language}),
  language:UiLanguage=getUiTextLanguage(),
): Promise<HistoryProductRead> {
  let hours: unknown;
  const history = await getExactHistorySnapshot(fromMs,toMs,async (from,to)=>{
    const raw=await request(from,to);
    if (!raw || typeof raw!=="object" || Array.isArray(raw)
      || new TextEncoder().encode(JSON.stringify(raw)).length>8*1024*1024) throw new Error("Invalid History product snapshot");
    const product=raw as Record<string,unknown>;
    hours=product.hours;
    return product.history;
  },language);
  const expected=new Map<AppCategory,number>();
  for (const session of history.sessions) {
    const category=session.confirmed!.category;
    const total=(expected.get(category)??0)+(session.duration??0);
    if (!Number.isSafeInteger(total)) throw new Error("History category quantity overflow");
    expected.set(category,total);
  }
  return {...history,hours:parseActivityHours(hours,expected)};
}
