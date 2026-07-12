/* global process */
import fs from "node:fs/promises";
import path from "node:path";
import { JSDOM } from "jsdom";
import { Readability } from "@mozilla/readability";
import createDOMPurify from "dompurify";
import TurndownService from "turndown";

const line = await new Promise((resolve) => { let data=""; process.stdin.setEncoding("utf8"); process.stdin.on("data",c=>data+=c); process.stdin.on("end",()=>resolve(data.trim())); });
const rpc=JSON.parse(line); const p=rpc.params; const root=path.resolve(p.projectRoot,p.stagingRoot); const input=path.resolve(root,p.chainedInput||"fetched.html");
if(!input.startsWith(root+path.sep)) throw new Error("input escaped staging");
const html=await fs.readFile(input,"utf8"); const dom=new JSDOM(html,{url:p.input.normalizedLocator||p.input.locator,runScripts:"outside-only",resources:undefined});
const article=new Readability(dom.window.document.cloneNode(true)).parse(); if(!article?.content) throw new Error("empty article");
const clean=createDOMPurify(dom.window).sanitize(article.content,{FORBID_TAGS:["script","style","iframe","object","embed","form"],FORBID_ATTR:["style"]});
const parsed=new JSDOM(clean,{url:dom.window.location.href}); for(const image of parsed.window.document.querySelectorAll("img")){image.removeAttribute("src");image.removeAttribute("srcset");image.removeAttribute("data-src");}
const persistedHtml=parsed.window.document.body.innerHTML; const markdown=new TurndownService({codeBlockStyle:"fenced",headingStyle:"atx"}).turndown(persistedHtml); await fs.writeFile(path.join(root,"candidate.md"),`# ${article.title||"Untitled"}\n\n${markdown}\n`); await fs.writeFile(path.join(root,"source.html"),persistedHtml); await fs.writeFile(path.join(root,"metadata.json"),JSON.stringify({title:article.title,byline:article.byline,imageRequests:[]}));
process.stdout.write(JSON.stringify({jsonrpc:"2.0",id:rpc.id,result:{sourceSnapshotPath:"source.html",markdownPath:"candidate.md",assetPaths:[],metadataPath:"metadata.json",title:article.title||"Untitled",textCoverage:1,warnings:[]},error:null})+"\n");
