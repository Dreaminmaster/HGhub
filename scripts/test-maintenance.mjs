// Real update UI, mocked native transport; no content service or external network.
import assert from "node:assert/strict";
import { createServer } from "vite";
import { chromium } from "playwright-core";
let vite,browser;
try {
  vite=await createServer({server:{host:"127.0.0.1",port:0,strictPort:false}});
  await vite.listen();
  const origin=vite.resolvedUrls.local[0];
  browser=await chromium.launch({headless:true,executablePath:process.env.BROWSER_EXECUTABLE||undefined});
  const page=await browser.newPage({userAgent:"Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36"});
  const external=[],errors=[];
  page.on("pageerror",e=>errors.push(e.message));
  await page.route("**/*",route=>{if(new URL(route.request().url()).origin===new URL(origin).origin)return route.continue();external.push(route.request().url());return route.abort();});
  await page.goto(origin+"tests/maintenance.html");
  await page.waitForFunction(()=>!!window.maintenanceFixture);
  await page.getByRole("button",{name:"检查更新",exact:true}).waitFor();
  await page.evaluate(()=>window.maintenanceFixture.check());
  await page.getByRole("button",{name:"检查中…",exact:true}).waitFor();
  assert.deepEqual(await page.evaluate(()=>window.maintenanceFixture.counts()),{checks:1,downloads:0,reveals:0});
  await page.evaluate(()=>window.maintenanceFixture.finishCheck());
  await page.getByText("Local test version",{exact:true}).waitFor();
  assert.equal((await page.evaluate(()=>window.maintenanceFixture.counts())).downloads,0,"checking must not download");
  await page.getByRole("button",{name:"下载安装包",exact:true}).click();
  await page.getByRole("button",{name:"下载并校验中…",exact:true}).waitFor();
  await page.evaluate(()=>{window.maintenanceFixture.downloadAgain();window.maintenanceFixture.switchChannel();window.maintenanceFixture.check();});
  assert.deepEqual(await page.evaluate(()=>window.maintenanceFixture.counts()),{checks:1,downloads:1,reveals:0});
  assert.equal(await page.locator("select").inputValue(),"stable");
  await page.getByText("0.5 / 1.0 MB",{exact:true}).waitFor();
  await page.evaluate(()=>window.maintenanceFixture.finishDownload());
  await page.getByText(/文件完整性校验通过/).waitFor();
  await page.getByRole("button",{name:/本地安装包与恢复记录/}).click();
  await page.getByText("v1.2.98",{exact:true}).waitFor();
  await page.getByRole("button",{name:"重新校验并定位文件",exact:true}).click();
  await page.getByText(/已重新验证文件并在 Finder 中定位/).waitFor();
  assert.equal((await page.evaluate(()=>window.maintenanceFixture.counts())).reveals,1);
  await page.evaluate(()=>window.maintenanceFixture.newDownload());
  await page.getByRole("button",{name:"下载安装包",exact:true}).click();
  await page.getByRole("button",{name:"下载并校验中…",exact:true}).waitFor();
  await page.evaluate(()=>window.maintenanceFixture.failDownload());
  await page.getByText("fixture download failed",{exact:true}).waitFor();
  assert.equal(await page.getByRole("button",{name:"下载安装包",exact:true}).isEnabled(),true);
  assert.equal(await page.getByText(/文件完整性校验通过/).count(),0,"failed download must not retain success");
  await page.evaluate(()=>window.maintenanceFixture.newCheck());
  await page.getByRole("button",{name:"检查更新",exact:true}).click();
  await page.evaluate(()=>window.maintenanceFixture.failCheck());
  await page.getByText("fixture unavailable",{exact:true}).waitFor();
  await page.getByRole("button",{name:"更新偏好备份",exact:true}).click();
  const field=page.getByRole("textbox",{name:"备份 JSON"});
  await page.getByRole("button",{name:"生成备份",exact:true}).click();
  const parsed=JSON.parse(await field.inputValue());
  assert.deepEqual(parsed,{schema:1,kind:"maintenance-preferences",preferences:{autoCheck:true,channel:"stable"}});
  await field.fill(JSON.stringify({...parsed,preferences:{autoCheck:false,channel:"beta",token:"unexpected"}}));
  await page.getByRole("button",{name:"检查备份",exact:true}).click();
  await page.getByText("备份格式无效或含有不支持的字段。",{exact:true}).waitFor();
  assert.equal(await page.locator("select").inputValue(),"stable");
  await field.fill(JSON.stringify({...parsed,preferences:{autoCheck:false,channel:"beta"}}));
  await page.getByRole("button",{name:"检查备份",exact:true}).click();
  await page.getByText("将恢复：含测试版；启动时不检查。",{exact:true}).waitFor();
  await page.getByRole("button",{name:"确认恢复偏好",exact:true}).click();
  await page.getByText("更新偏好已恢复。",{exact:true}).waitFor();
  assert.equal(await page.locator("select").inputValue(),"beta");
  assert.equal((await page.evaluate(()=>window.maintenanceFixture.counts())).checks,2,"restore must not trigger a network check");
  // Read-only status and malformed metadata must not expose a download action.
  for (const [method, message] of [
    ["noRelease","没有找到此渠道中带有效文件摘要的 macOS 安装包。"],
    ["current","当前版本不低于此渠道可下载版本。"],
    ["invalidDigest","更新文件信息不完整或无效"],
    ["unknownStatus","更新状态无法识别"]
  ]) {
    await page.evaluate(()=>window.maintenanceFixture.newCheck());
    await page.getByRole("button",{name:"检查更新",exact:true}).click();
    await page.evaluate(m=>window.maintenanceFixture[m](),method);
    await page.getByText(message,{exact:true}).waitFor();
    assert.equal(await page.getByRole("button",{name:"下载安装包",exact:true}).count(),0);
    await page.evaluate(()=>window.maintenanceFixture.downloadAgain());
    assert.equal((await page.evaluate(()=>window.maintenanceFixture.counts())).downloads,2);
  }
  await page.reload();
  await page.getByRole("button",{name:"检查更新",exact:true}).waitFor();
  assert.equal(await page.locator("select").inputValue(),"beta","restored channel survives reload");
  assert.equal((await page.evaluate(()=>window.maintenanceFixture.counts())).downloads,0);
  assert.deepEqual(errors,[]);
  assert.deepEqual(external,[]);
  console.log("PASS: update check, deduplication, explicit download, progress, local archive, failure and no external requests");
}finally{await browser?.close();await vite?.close();}
