// Injected by the opt-in Rust test only, into the real built application.
(async () => {
  if (window.__heatmapAcceptanceRunning) return;
  window.__heatmapAcceptanceRunning = true;
  const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
  const report = data => fetch('/__acceptance', {method:'POST', headers:{'Content-Type':'application/json'}, body:JSON.stringify(data)});
  const wait = async (test, label) => {
    for (let i = 0; i < 600; i++) { if (test()) return; await delay(100); }
    throw new Error(`Timed out: ${label}`);
  };
  const round = Number(sessionStorage.getItem('heatmapAcceptanceRound') || '0') + 1;
  // WebView recreation can reset sessionStorage; the server assigns the final round.
  const calls = [];
  const original = window.fetch;
  window.fetch = async function(input, options) {
    const started = performance.now();
    const response = await original.call(this, input, options);
    if (String(input).endsWith('/cmd_get_daily_activity')) {
      const value = await response.clone().json();
      if (!Array.isArray(value.days)) throw new Error('daily IPC returned an error');
      const args = JSON.parse(options.body);
      calls.push({from:args.from,to:args.to,days:value.days.length,
        total_ms:value.days.reduce((total, day) => total + day.active_ms,0),
        bytes:new TextEncoder().encode(JSON.stringify(value)).length,elapsed_ms:performance.now()-started});
    }
    return response;
  };
  const errors = [];
  window.addEventListener('error', event => errors.push(String(event.message)));
  window.addEventListener('unhandledrejection', event => errors.push(String(event.reason)));
  try {
    const nav = name => [...document.querySelectorAll('button[aria-label]')].find(el => [name, name === 'Data' ? '\u6570\u636e' : '\u5386\u53f2'].includes(el.getAttribute('aria-label')));
    await wait(() => nav('Data'), 'navigation');
    await delay(5000);
    await report({phase:'dashboard',round});
    nav('Data').click();
    await wait(() => calls.some(call => call.total_ms === 3000000000) && document.querySelector('.data-heatmap-cell[style*="--heatmap-intensity: 1"]'), 'populated heatmap and real daily IPC');
    if (document.querySelector('.data-heatmap-panel [role="alert"]')) throw new Error('heatmap error state');
    await delay(3000);
    await report({phase:'heatmap',round,calls});
    const yesterday = new Date(); yesterday.setDate(yesterday.getDate()-1);
    const key = [yesterday.getFullYear(),String(yesterday.getMonth()+1).padStart(2,'0'),String(yesterday.getDate()).padStart(2,'0')].join('-');
    const cell = document.querySelector(`[data-history-date="${key}"]`);
    if (!cell) throw new Error('missing yesterday');
    cell.dispatchEvent(new MouseEvent('dblclick',{bubbles:true,cancelable:true,view:window}));
    await wait(() => nav('History')?.className.includes('qp-nav-item-active'), 'history navigation');
    await wait(() => document.body.innerText.includes('Fixture App'), 'history fixture records');
    const overflow = document.documentElement.scrollWidth > window.innerWidth + 2;
    if (overflow || errors.length) throw new Error(JSON.stringify({overflow,errors}));
    sessionStorage.setItem('heatmapAcceptanceRound',String(round));
    await report({phase:'complete',passed:true,calls,history_date:key,overflow,errors});
  } catch (error) {
    await report({phase:'complete',passed:false,error:String(error),calls,errors});
  }
})();
