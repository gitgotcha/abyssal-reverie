// DOM-level checks only; these do not substitute for browser visual QA.
const {JSDOM}=require('jsdom');
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const dom=new JSDOM(fs.readFileSync(path.join(__dirname,'index.html'),'utf8'),{runScripts:'outside-only',url:'http://localhost/'}),w=dom.window,d=w.document;
w.HTMLDialogElement.prototype.showModal=function(){this.open=true};w.HTMLDialogElement.prototype.close=function(){this.open=false};
for(const f of ['assets/feather.min.js','assets/pinyin.js','app.js'])w.eval(fs.readFileSync(path.join(__dirname,f),'utf8'));
let count=0;function test(name,fn){fn();count++;console.log('PASS '+name)}
const click=s=>{assert.ok(d.querySelector(s),'missing '+s);d.querySelector(s).click()},input=(s,v,type='input')=>{const el=d.querySelector(s);el.value=v;el.dispatchEvent(new w.Event(type,{bubbles:true}))},text=()=>d.body.textContent;
test('initial task and budget',()=>assert.match(text(),/本任务按 1 分钟/));
test('pinyin keyword search',()=>{input('#search','lunwen');assert.match(d.querySelector('.list').textContent,/阅读论文/);assert.doesNotMatch(d.querySelector('.list').textContent,/背单词/)});
test('initials search',()=>{input('#search','bdc');assert.match(d.querySelector('.list').textContent,/背单词/)});
test('empty search and clear',()=>{input('#search','zzzzzz');assert.match(text(),/没有找到/);click('[data-action="clear-search"]');assert.match(d.querySelector('.list').textContent,/背单词/)});
test('project view with associated tasks',()=>{click('[data-category="projects"]');assert.match(d.querySelector('.detail').textContent,/确定研究主题/)});
test('cannot complete unfinished project',()=>{click('[data-action="complete-project"]');assert.match(d.querySelector('#toast').textContent,/还有未完成任务/)});
test('new project can be created',()=>{click('[data-action="create"]');input('[name="name"]','测试项目');d.querySelector('#edit-form').dispatchEvent(new w.Event('submit',{bubbles:true,cancelable:true}));assert.match(d.querySelector('.list').textContent,/测试项目/)});
test('duplicate project rejected',()=>{click('[data-action="create"]');input('[name="name"]','测试项目');d.querySelector('#edit-form').dispatchEvent(new w.Event('submit',{bubbles:true,cancelable:true}));assert.match(d.querySelector('#form-error').textContent,/已有同名/);click('#cancel-dialog')});
test('new task is unassigned until explicitly linked',()=>{click('[data-action="project-task"]');input('[name="name"]','新增测试任务');d.querySelector('#edit-form').dispatchEvent(new w.Event('submit',{bubbles:true,cancelable:true}));assert.match(d.querySelector('.detail').textContent,/独立任务/)});
test('settings change preserves old budget',()=>{click('[data-nav="settings"]');input('#focus-minutes','40','change');click('[data-nav="management"]');click('[data-category="tasks"]');click('[data-task="t1"]');assert.match(d.querySelector('.detail').textContent,/本任务按 1 分钟/)});
test('new task uses latest focus duration',()=>{click('[data-action="create"]');input('[name="name"]','40分钟任务');d.querySelector('#edit-form').dispatchEvent(new w.Event('submit',{bubbles:true,cancelable:true}));assert.match(d.querySelector('.detail').textContent,/本任务按 40 分钟/)});
test('searchable project selection',()=>{click('[data-action="pick-project"]');input('#picker-search','bylw');assert.match(d.querySelector('#picker-list').textContent,/毕业论文/);click('[data-pick="p1"]');assert.match(d.querySelector('.detail').textContent,/毕业论文/)});
test('task completion supports undo',()=>{click('[data-check="t1"]');assert.match(d.querySelector('#toast').textContent,/任务已完成/);click('[data-action="undo"]');assert.ok(d.querySelector('[data-check="t1"]')&&!d.querySelector('[data-check="t1"]').checked)});
test('tag delete previews and retains tasks',()=>{click('[data-category="tags"]');click('[data-action="delete-tag"]');assert.match(d.querySelector('#modal').textContent,/任务及投入记录都会保留/);click('#confirm-dialog');assert.match(d.querySelector('#toast').textContent,/任务保留/);click('[data-action="undo"]')});
test('focus start pause completion undo',()=>{click('[data-nav="management"]');click('[data-category="tasks"]');click('[data-task="t1"]');click('[data-action="start-task"]');assert.match(d.querySelector('.focus-page').textContent,/暂停/);click('[data-action="toggle-timer"]');assert.match(d.querySelector('.focus-page').textContent,/开始 \/ 继续/);click('[data-action="finish-task"]');assert.match(d.querySelector('#toast').textContent,/真实投入保留/);click('[data-action="undo"]')});
console.log(count+' checks passed (DOM only).');w.close();process.exit(0);
