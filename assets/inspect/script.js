const sea=document.getElementById('sea');
for(let i=0;i<14;i++){const b=document.createElement('span');const s=6+((i*7)%26);
  b.style.width=b.style.height=s+'px';b.style.left=((i*67)%100)+'%';
  b.style.animationDuration=(11+(i%6)*3)+'s';b.style.animationDelay=(-(i*1.7))+'s';sea.appendChild(b);}

const rows=STEPS;
const v=document.getElementById('vid');
const prog=document.getElementById('prog');
const knob=document.getElementById('knob');
const tc=document.getElementById('tc');
const play=document.getElementById('play');
const rail=document.getElementById('rail');
const cont=document.getElementById('steps');
const now=document.getElementById('now');
const speedBtn=document.getElementById('speed');
const loopBtn=document.getElementById('loop');
const fsBtn=document.getElementById('fs');
const watch=document.getElementById('watch');
const lightbox=document.getElementById('lightbox');
const lbImg=document.getElementById('lb-img');
const lbCap=document.getElementById('lb-cap');

// Duration from metadata; replaced by the real MP4 duration once known.
let DUR=TOTAL;

rows.forEach((r,i)=>{
  const step=document.createElement('button');step.type='button';step.className='step';
  step.title=r.name+' · frames '+r.frames;
  const num=document.createElement('span');num.className='num';num.textContent=r.n;
  const mini=document.createElement('div');mini.className='mini';
  if(r.thumb){const img=document.createElement('img');img.src=r.thumb;img.alt='';mini.appendChild(img);
    mini.classList.add('zoom');mini.title='Enlarge screenshot';
    mini.addEventListener('click',e=>{e.stopPropagation();openLightbox(r);});}
  else{mini.classList.add('ph','k-'+r.kind);mini.textContent=r.kind;}
  const body=document.createElement('div');body.className='stepbody';
  const nm=document.createElement('div');nm.className='nm';nm.textContent=r.name+' ';
  const kd=document.createElement('span');kd.className='kd k-'+r.kind;kd.textContent=r.kind;nm.appendChild(kd);
  const mt=document.createElement('div');mt.className='mt';mt.textContent=r.note;
  body.appendChild(nm);body.appendChild(mt);
  const jump=document.createElement('div');jump.className='jump';jump.textContent='▶ '+r.t.toFixed(1)+'s';
  step.append(num,mini,body,jump);
  step.addEventListener('click',()=>seek(i));
  cont.appendChild(step);
});

// Step boundary notches on the scrubber rail.
const ticks=rows.map((r,i)=>{
  const t=document.createElement('div');t.className='tick';t.title=r.n+' · '+r.name;
  t.addEventListener('pointerdown',e=>e.stopPropagation());
  t.addEventListener('click',e=>{e.stopPropagation();seek(i);});
  rail.appendChild(t);return t;
});
function placeTicks(){ticks.forEach((el,i)=>{el.style.left=(DUR?rows[i].t/DUR*100:0)+'%';});}

function paint(t){const p=DUR?Math.min(100,t/DUR*100):0;prog.style.width=p+'%';knob.style.left=p+'%';
  tc.textContent=t.toFixed(1)+' / '+DUR.toFixed(1)+'s';
  rail.setAttribute('aria-valuenow',t.toFixed(1));
  rail.setAttribute('aria-valuetext',t.toFixed(1)+' of '+DUR.toFixed(1)+' seconds');}
function mark(t){let idx=0;rows.forEach((r,j)=>{if(t>=r.t-0.02)idx=j;});
  document.querySelectorAll('.step').forEach((s,j)=>s.classList.toggle('active',j===idx));
  const r=rows[idx];if(now)now.textContent=r?r.n+' · '+r.name:'';}
function refresh(){if(v){paint(v.currentTime);mark(v.currentTime);}}
function seek(i){seekTo(rows[i].t);}
function seekTo(t){if(!v)return;v.currentTime=Math.max(0,Math.min(DUR||0,t));refresh();}
function toggle(){if(v)v.paused?v.play():v.pause();}

if(v){
  // Smooth progress while playing; timeupdate alone only ticks ~4x/second.
  let rafId=0;
  const tickLoop=()=>{refresh();rafId=requestAnimationFrame(tickLoop);};
  v.addEventListener('play',()=>{play.textContent='❚❚';cancelAnimationFrame(rafId);tickLoop();});
  v.addEventListener('pause',()=>{play.textContent='▶';cancelAnimationFrame(rafId);refresh();});
  v.addEventListener('ended',()=>{if(!v.loop)play.textContent='↻';});
  v.addEventListener('seeked',refresh);
  v.addEventListener('loadedmetadata',()=>{
    if(isFinite(v.duration)&&v.duration>0)DUR=v.duration;
    rail.setAttribute('aria-valuemax',DUR.toFixed(1));placeTicks();refresh();});
  play.addEventListener('click',toggle);
  v.addEventListener('click',toggle);
  v.addEventListener('dblclick',toggleFs);

  // Drag anywhere on the rail to scrub.
  let dragging=false;
  const railSeek=e=>{const rct=rail.getBoundingClientRect();
    seekTo((e.clientX-rct.left)/rct.width*DUR);};
  rail.addEventListener('pointerdown',e=>{dragging=true;rail.setPointerCapture(e.pointerId);railSeek(e);});
  rail.addEventListener('pointermove',e=>{if(dragging)railSeek(e);});
  rail.addEventListener('pointerup',()=>{dragging=false;});
  rail.addEventListener('pointercancel',()=>{dragging=false;});

  const SPEEDS=[1,0.5,0.25,2];const SPEED_LABELS=['1×','½×','¼×','2×'];let si=0;
  speedBtn.addEventListener('click',()=>{si=(si+1)%SPEEDS.length;
    v.playbackRate=SPEEDS[si];speedBtn.textContent=SPEED_LABELS[si];
    speedBtn.classList.toggle('on',si!==0);});
  loopBtn.addEventListener('click',toggleLoop);
  fsBtn.addEventListener('click',toggleFs);

  document.addEventListener('keydown',e=>{
    const tag=e.target.tagName||'';
    if(/^(INPUT|TEXTAREA|SELECT)$/.test(tag))return;
    if(e.key==='Escape'){closeLightbox();return;}
    if(!lightbox.hidden)return;
    if(e.code==='Space'){
      // Let focused buttons/links keep their native Space behavior.
      if(/^(BUTTON|A|SUMMARY)$/.test(tag))return;
      e.preventDefault();toggle();}
    else if(e.key==='ArrowLeft'){e.preventDefault();seekTo(v.currentTime-1);}
    else if(e.key==='ArrowRight'){e.preventDefault();seekTo(v.currentTime+1);}
    else if(e.key===','){v.pause();seekTo(v.currentTime-1/FPS);}
    else if(e.key==='.'){v.pause();seekTo(v.currentTime+1/FPS);}
    else if(e.key==='f'||e.key==='F'){toggleFs();}
    else if(e.key==='l'||e.key==='L'){toggleLoop();}
    else if(e.key==='Home'){e.preventDefault();seekTo(0);}
    else if(e.key==='End'){e.preventDefault();seekTo(DUR);}
  });
}

function toggleFs(){
  if(document.fullscreenElement){document.exitFullscreen();}
  else if(watch&&watch.requestFullscreen){watch.requestFullscreen();}
}
function toggleLoop(){if(!v)return;v.loop=!v.loop;
  loopBtn.classList.toggle('on',v.loop);loopBtn.setAttribute('aria-pressed',String(v.loop));
  if(v.loop&&v.ended)v.play();}

function openLightbox(r){lbImg.src=r.thumb;lbCap.textContent=r.n+' · '+r.name;lightbox.hidden=false;}
function closeLightbox(){lightbox.hidden=true;}
lightbox.addEventListener('click',closeLightbox);

document.querySelectorAll('.copy').forEach(btn=>{
  btn.addEventListener('click',()=>{
    const el=document.getElementById(btn.dataset.copy);
    const text=el?el.textContent:'';
    const done=()=>{btn.classList.add('ok');btn.textContent='copied';
      setTimeout(()=>{btn.classList.remove('ok');btn.textContent='copy';},1200);};
    if(navigator.clipboard&&navigator.clipboard.writeText){
      navigator.clipboard.writeText(text).then(done).catch(()=>copyFallback(text,done));}
    else copyFallback(text,done);
  });
});
function copyFallback(text,done){
  const ta=document.createElement('textarea');ta.value=text;
  ta.style.position='fixed';ta.style.opacity='0';document.body.appendChild(ta);
  ta.select();try{if(document.execCommand('copy'))done();}catch(_){/* leave button as-is */}
  ta.remove();
}

placeTicks();
paint(0);
mark(0);
