const {JSDOM}=require('jsdom');
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const dom=new JSDOM(fs.readFileSync(path.join(__dirname,'index.html'),'utf8'),{runScripts:'outside-only'}),w=dom.window,d=w.document;
w.HTMLDialogElement.prototype.showModal=function(){this.open=true};w.HTMLDialogElement.prototype.close=function(){this.open=false};
for(const f of ['assets/feather.min.js','assets/pinyin.js','app.js'])w.eval(fs.readFileSync(path.join(__dirname,f),'utf8')+(f==='app.js'?';window.testData=()=>data;window.testTimer=()=>timer;':''));
const click=s=>{assert.ok(d.querySelector(s),s);d.querySelector(s).click()};
try{
assert.ok(d.querySelector('#sidebar [data-nav="management"]'),'main sidebar must contain management');
assert.equal(d.querySelector('#sidebar [data-nav="tasks"]'),null,'tasks must not be a main sidebar item');
assert.equal(d.querySelectorAll('[data-category]').length,3);
click('[data-action="create"]');const name=d.querySelector('[name="name"]');name.value='只有标题的任务';d.querySelector('#edit-form').dispatchEvent(new w.Event('submit',{bubbles:true,cancelable:true}));
const task=w.testData().tasks.at(-1);assert.equal(task.project,'');assert.equal(task.tag,'');assert.equal(task.priority,'');assert.equal(task.date,'');assert.equal(task.notes,'');
click('[data-action="pick-tag"]');click('[data-pick="g1"]');assert.equal(task.tag,'g1');click('[data-action="pick-tag"]');click('[data-pick=""]');assert.equal(task.tag,'');
click('[data-category="projects"]');assert.match(d.querySelector('#workspace').textContent,/毕业论文/);
click('[data-nav="settings"]');click('[data-nav="management"]');assert.ok(d.querySelector('[data-category="projects"].active'),'return preserves last category');
console.log('PASS navigation, three nested categories, nullable task fields, optional tag assignment/removal, remembered category');
click('[data-category="tasks"]');click('[data-filter="done"]');click('[data-task="t4"]');click('[data-action="archive-task"]');click('[data-filter="archived"]');click('[data-task="t4"]');click('[data-action="archive-task"]');assert.equal(w.testData().tasks.find(t=>t.id==='t4').status,'done');
click('[data-filter="todo"]');click('[data-task="t1"]');click('[data-action="start-task"]');click('[data-nav="management"]');click('[data-check="t1"]');assert.equal(w.testTimer().running,false);assert.equal(w.testTimer().task,null);click('[data-action="undo"]');assert.equal(w.testData().tasks.find(t=>t.id==='t1').status,'todo');
console.log('PASS completed-task archive/restore and active-task completion/unbinding/undo');
}finally{w.close();}
process.exit(0);
