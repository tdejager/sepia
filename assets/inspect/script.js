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

rows.forEach((r,i)=>{
  const step=document.createElement('div');step.className='step';
  const num=document.createElement('span');num.className='num';num.textContent=r.n;
  const mini=document.createElement('div');mini.className='mini';
  if(r.thumb){const img=document.createElement('img');img.src=r.thumb;img.alt='';mini.appendChild(img);}
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

function paint(t){const p=TOTAL?Math.min(100,t/TOTAL*100):0;prog.style.width=p+'%';knob.style.left=p+'%';
  tc.textContent=t.toFixed(1)+' / '+TOTAL.toFixed(1)+'s';}
function mark(t){let idx=0;rows.forEach((r,j)=>{if(t>=r.t-0.02)idx=j;});
  document.querySelectorAll('.step').forEach((s,j)=>s.classList.toggle('active',j===idx));}
function seek(i){if(v)v.currentTime=rows[i].t;
  document.querySelectorAll('.step').forEach((s,j)=>s.classList.toggle('active',j===i));}

if(v){
  v.addEventListener('timeupdate',()=>{paint(v.currentTime);mark(v.currentTime);});
  v.addEventListener('play',()=>{play.textContent='❚❚';});
  v.addEventListener('pause',()=>{play.textContent='▶';});
  play.addEventListener('click',()=>{v.paused?v.play():v.pause();});
  rail.addEventListener('click',e=>{const rct=rail.getBoundingClientRect();
    v.currentTime=Math.max(0,Math.min(TOTAL,(e.clientX-rct.left)/rct.width*TOTAL));});
  document.addEventListener('keydown',e=>{if(e.code==='Space'&&e.target.tagName!=='TEXTAREA'){e.preventDefault();v.paused?v.play():v.pause();}});
}
paint(0);
const first=document.querySelector('.step');if(first)first.classList.add('active');