document.documentElement.classList.add("js");

// Tour: a sidebar like the app's, crossfading between pages.
const captions = [
  "Type “coffee 4.50 @Blue Bottle yesterday” and it’s filed. Or import your bank’s CSV, Excel or OFX.",
  "Monthly budgets with a pacing line that shows where you should be today.",
  "Any period you like, with charts that animate and answer when you hover.",
  "Cards, savings, cash and investments, each in its own currency.",
  "Bills, subscriptions and paychecks that add themselves when they’re due.",
  "Set a target and see if you’re on track, and how much a month gets you there.",
];
const tabs = [...document.querySelectorAll(".tabs button")];
const frames = [...document.querySelectorAll(".frames img")];
const caption = document.querySelector(".caption");
function show(i) {
  tabs.forEach((t, j) => t.setAttribute("aria-selected", String(i === j)));
  frames.forEach((f, j) => f.classList.toggle("on", i === j));
  caption.textContent = captions[i];
}
tabs.forEach((t, i) => t.addEventListener("click", () => show(i)));
document.querySelector(".tabs").addEventListener("keydown", (e) => {
  const i = tabs.findIndex((t) => t.getAttribute("aria-selected") === "true");
  const d = { ArrowDown: 1, ArrowRight: 1, ArrowUp: -1, ArrowLeft: -1 }[e.key];
  if (!d) return;
  e.preventDefault();
  const n = (i + d + tabs.length) % tabs.length;
  show(n);
  tabs[n].focus();
});
show(0);

// Install: pick the visitor's platform, copy on click.
function pick(os) {
  document.querySelectorAll(".seg button").forEach((b) => b.setAttribute("aria-selected", String(b.dataset.os === os)));
  document.querySelectorAll(".installer [data-os]:not(button)").forEach((el) => (el.hidden = el.dataset.os !== os));
}
document.querySelectorAll(".seg button").forEach((b) => b.addEventListener("click", () => pick(b.dataset.os)));
if (/Win/i.test(navigator.userAgentData?.platform || navigator.platform || navigator.userAgent)) pick("win");

document.querySelectorAll(".copy").forEach((btn) => {
  btn.addEventListener("click", async () => {
    const text = btn.parentElement.querySelector("code").textContent;
    try {
      await navigator.clipboard.writeText(text);
      btn.textContent = "Copied";
    } catch {
      const r = document.createRange();
      r.selectNodeContents(btn.parentElement.querySelector("code"));
      getSelection().removeAllRanges();
      getSelection().addRange(r);
      btn.textContent = "Selected";
    }
    clearTimeout(btn._t);
    btn._t = setTimeout(() => (btn.textContent = "Copy"), 1600);
  });
});

// Decks: the current card in front, its neighbours half showing behind it.
const still = matchMedia("(prefers-reduced-motion: reduce)").matches;
document.querySelectorAll(".deck").forEach((deck) => {
  const cards = [...deck.querySelectorAll(".card")];
  const n = cards.length;
  const label = deck.nextElementSibling?.classList.contains("deck-label") ? deck.nextElementSibling : null;
  const auto = Number(deck.dataset.auto) || 0;
  let at = 0;
  let timer;
  function lay() {
    cards.forEach((c, i) => {
      // Offset from the front card, wrapped so the deck is a loop.
      const d = ((i - at + n + Math.floor(n / 2)) % n) - Math.floor(n / 2);
      const a = Math.abs(d);
      const scale = a === 0 ? 1 : a === 1 ? 0.86 : 0.74;
      c.style.transform = `translateX(${d * (a > 1 ? 70 : 50)}%) scale(${scale})`;
      c.style.zIndex = String(30 - a * 10);
      c.style.opacity = a === 0 ? "1" : a === 1 ? "0.5" : "0";
      c.style.filter = a === 0 ? "none" : "blur(1.5px)";
      c.classList.toggle("back", a !== 0);
      c.setAttribute("aria-hidden", String(a !== 0));
    });
    if (label) {
      const c = cards[at];
      label.querySelector("span").textContent = c.dataset.name;
    }
  }
  function go(step) {
    at = (at + step + n) % n;
    lay();
    restart();
  }
  function restart() {
    clearTimeout(timer);
    if (auto && !still) timer = setTimeout(() => !document.hidden && go(1), auto);
  }
  deck.querySelector(".prev").addEventListener("click", () => go(-1));
  deck.querySelector(".next").addEventListener("click", () => go(1));
  if (auto) {
    deck.addEventListener("mouseenter", () => clearTimeout(timer));
    deck.addEventListener("mouseleave", restart);
    document.addEventListener("visibilitychange", restart);
  }
  lay();
  restart();
});

// In-page links glide to their section instead of jumping, and stop if you scroll yourself.
let glide = 0;
function glideTo(target) {
  const barH = document.querySelector(".top").offsetHeight;
  const from = scrollY;
  // Land a section's heading just under the top bar, so the whole section is in view.
  const anchor = target.querySelector(":scope > h2") || target;
  const gap = anchor === target ? 8 : 20;
  const to = Math.max(0, Math.min(anchor.getBoundingClientRect().top + from - barH - gap, document.documentElement.scrollHeight - innerHeight));
  const dist = to - from;
  if (still || Math.abs(dist) < 2) return scrollTo({ top: to, behavior: "instant" });
  const ms = Math.min(1300, 550 + Math.abs(dist) * 0.25);
  const ease = (t) => (t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2);
  const id = ++glide;
  const t0 = performance.now();
  const step = (now) => {
    if (id !== glide) return;
    const t = Math.min(1, (now - t0) / ms);
    scrollTo({ top: from + dist * ease(t), behavior: "instant" });
    if (t < 1) requestAnimationFrame(step);
  };
  requestAnimationFrame(step);
}
["wheel", "touchstart", "keydown"].forEach((ev) => addEventListener(ev, () => glide++, { passive: true }));
document.addEventListener("click", (e) => {
  const a = e.target.closest('a[href^="#"]');
  if (!a || e.metaKey || e.ctrlKey || e.shiftKey || e.button !== 0) return;
  const id = a.getAttribute("href").slice(1);
  const target = id ? document.getElementById(id) : document.body;
  if (!target) return;
  e.preventDefault();
  history.pushState(null, "", id ? `#${id}` : location.pathname);
  glideTo(target);
});

// Soft entrance on scroll; a quiet border on the top bar once you've moved.
const io = new IntersectionObserver(
  (entries) => entries.forEach((e) => e.isIntersecting && (e.target.classList.add("in"), io.unobserve(e.target))),
  { rootMargin: "0px 0px -8% 0px" },
);
document.querySelectorAll(".reveal").forEach((el) => io.observe(el));
const bar = document.querySelector(".top");
addEventListener("scroll", () => bar.classList.toggle("scrolled", scrollY > 8), { passive: true });
