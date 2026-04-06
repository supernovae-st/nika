"use strict";var m=Object.create;var p=Object.defineProperty;var w=Object.getOwnPropertyDescriptor;var f=Object.getOwnPropertyNames;var k=Object.getPrototypeOf,y=Object.prototype.hasOwnProperty;var D=(i,e)=>{for(var t in e)p(i,t,{get:e[t],enumerable:!0})},h=(i,e,t,s)=>{if(e&&typeof e=="object"||typeof e=="function")for(let a of f(e))!y.call(i,a)&&a!==t&&p(i,a,{get:()=>e[a],enumerable:!(s=w(e,a))||s.enumerable});return i};var l=(i,e,t)=>(t=i!=null?m(k(i)):{},h(e||!i||!i.__esModule?p(t,"default",{value:i,enumerable:!0}):t,i)),U=i=>h(p({},"__esModule",{value:!0}),i);var G={};D(G,{activate:()=>x,deactivate:()=>M});module.exports=U(G);var r=l(require("vscode"));var n=l(require("vscode")),g=l(require("crypto"));function v(){return g.randomBytes(16).toString("base64")}var o=class i{constructor(e,t,s){this.extensionUri=e;this.onNodeClicked=t;this.onNodeDoubleClicked=s;this.disposables=[]}static{this.viewType="nika.dagView"}show(e){if(this.panel){this.panel.reveal(n.ViewColumn.Beside),e&&this.loadGraph(e);return}this.panel=n.window.createWebviewPanel(i.viewType,"Nika DAG",{viewColumn:n.ViewColumn.Beside,preserveFocus:!0},{enableScripts:!0,retainContextWhenHidden:!0,localResourceRoots:[n.Uri.joinPath(this.extensionUri,"dist","webview")]}),this.panel.iconPath={light:n.Uri.joinPath(this.extensionUri,"media","dag-light.svg"),dark:n.Uri.joinPath(this.extensionUri,"media","dag-dark.svg")},this.panel.webview.html=this.getHtml(this.panel.webview),this.panel.webview.onDidReceiveMessage(t=>this.handleMessage(t),null,this.disposables),this.panel.onDidDispose(()=>{this.panel=void 0,this.disposables.forEach(t=>t.dispose()),this.disposables=[]},null,this.disposables),this.panel.onDidChangeViewState(t=>{t.webviewPanel.visible&&this.currentGraph&&this.postMessage({kind:"dag:load",graph:this.currentGraph})},null,this.disposables),this.disposables.push(n.window.onDidChangeActiveColorTheme(()=>{this.postMessage({kind:"theme:changed"})})),e&&(this.currentGraph=e)}loadGraph(e){this.currentGraph=e,this.panel?.visible&&this.postMessage({kind:"dag:load",graph:e})}updateTaskStatus(e,t,s){if(this.currentGraph){let a=this.currentGraph.nodes.find(c=>c.id===e);a&&(a.status=t,a.durationMs=s)}this.postMessage({kind:"dag:updateStatus",taskId:e,status:t,durationMs:s})}batchUpdateStatus(e){if(this.currentGraph)for(let t of e){let s=this.currentGraph.nodes.find(a=>a.id===t.taskId);s&&(s.status=t.status,s.durationMs=t.durationMs)}this.postMessage({kind:"dag:batchUpdateStatus",updates:e})}fitToView(){this.postMessage({kind:"dag:fitToView"})}clear(){this.currentGraph=void 0,this.postMessage({kind:"dag:clear"})}dispose(){this.panel?.dispose()}postMessage(e){this.panel?.webview.postMessage(e)}handleMessage(e){switch(e.kind){case"dag:ready":this.currentGraph&&this.postMessage({kind:"dag:load",graph:this.currentGraph});break;case"dag:nodeClicked":this.onNodeClicked?.(e.taskId);break;case"dag:nodeDoubleClicked":this.onNodeDoubleClicked?.(e.taskId);break;case"dag:requestRefresh":this.currentGraph&&this.postMessage({kind:"dag:load",graph:this.currentGraph});break;case"dag:viewportChanged":break}}getHtml(e){let t=v(),s=e.asWebviewUri(n.Uri.joinPath(this.extensionUri,"dist","webview","dag.js")),a=e.asWebviewUri(n.Uri.joinPath(this.extensionUri,"dist","webview","dag.css"));return`<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <meta http-equiv="Content-Security-Policy" content="${["default-src 'none'",`style-src ${e.cspSource} 'unsafe-inline'`,`script-src 'nonce-${t}'`,`img-src ${e.cspSource} data:`,`font-src ${e.cspSource}`].join("; ")}">
  <link rel="stylesheet" href="${a}">
  <title>Nika DAG</title>
</head>
<body>
  <div id="dag-toolbar">
    <button id="btn-fit" title="Fit to view (F)">Fit</button>
    <button id="btn-zoom-in" title="Zoom in (+)">+</button>
    <button id="btn-zoom-out" title="Zoom out (-)">-</button>
    <span id="dag-title"></span>
    <span id="dag-status"></span>
  </div>
  <div id="dag-container"></div>
  <script nonce="${t}" src="${s}"></script>
</body>
</html>`}},u=class{constructor(e){this.extensionUri=e}async deserializeWebviewPanel(e,t){let s=v(),a=e.webview.asWebviewUri(n.Uri.joinPath(this.extensionUri,"dist","webview","dag.js")),c=e.webview.asWebviewUri(n.Uri.joinPath(this.extensionUri,"dist","webview","dag.css")),b=["default-src 'none'",`style-src ${e.webview.cspSource} 'unsafe-inline'`,`script-src 'nonce-${s}'`,`img-src ${e.webview.cspSource} data:`,`font-src ${e.webview.cspSource}`].join("; ");e.webview.html=`<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <meta http-equiv="Content-Security-Policy" content="${b}">
  <link rel="stylesheet" href="${c}">
  <title>Nika DAG</title>
</head>
<body>
  <div id="dag-toolbar">
    <button id="btn-fit" title="Fit to view (F)">Fit</button>
    <button id="btn-zoom-in" title="Zoom in (+)">+</button>
    <button id="btn-zoom-out" title="Zoom out (-)">-</button>
    <span id="dag-title"></span>
    <span id="dag-status"></span>
  </div>
  <div id="dag-container"></div>
  <script nonce="${s}" src="${a}"></script>
</body>
</html>`}};var d;function x(i){i.subscriptions.push(r.window.registerWebviewPanelSerializer(o.viewType,new u(i.extensionUri))),i.subscriptions.push(r.commands.registerCommand("nika.showDag",()=>{d||(d=new o(i.extensionUri,t=>{r.window.showInformationMessage(`Task: ${t}`)},t=>{r.commands.executeCommand("nika.showTaskOutput",t)}));let e={workflowName:"research-and-summarize",nodes:[{id:"research",label:"research",verb:"infer",status:"success",durationMs:2340,provider:"anthropic",model:"claude-sonnet-4-6",dependsOn:[]},{id:"scrape",label:"scrape",verb:"fetch",status:"success",durationMs:890,dependsOn:["research"]},{id:"transform",label:"transform",verb:"invoke",status:"running",dependsOn:["scrape"]},{id:"summarize",label:"summarize",verb:"infer",status:"pending",provider:"anthropic",dependsOn:["transform"]},{id:"format",label:"format",verb:"exec",status:"pending",dependsOn:["summarize"]},{id:"notify",label:"notify",verb:"fetch",status:"pending",dependsOn:["format"]}],edges:[{id:"e1",source:"research",target:"scrape",isDataEdge:!0},{id:"e2",source:"scrape",target:"transform",isDataEdge:!0},{id:"e3",source:"transform",target:"summarize",isDataEdge:!0},{id:"e4",source:"summarize",target:"format",isDataEdge:!0},{id:"e5",source:"format",target:"notify",isDataEdge:!1}]};d.show(e)}))}function M(){d?.dispose(),d=void 0}0&&(module.exports={activate,deactivate});
