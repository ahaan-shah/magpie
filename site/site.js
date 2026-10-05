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

// Install: show the visitor's platform; a quiet link switches.
function pick(os) {
  document.querySelectorAll(".installer [data-os]").forEach((el) => (el.hidden = el.dataset.os !== os));
}
document.querySelectorAll(".switch").forEach((b) => b.addEventListener("click", () => pick(b.dataset.os === "win" ? "unix" : "win")));
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

// Testimonials: slide left every few seconds; click for the next one.
const track = document.querySelector(".track");
const slides = [...track.children];
const dots = [...document.querySelectorAll(".dots button")];
track.appendChild(slides[0].cloneNode(true)).setAttribute("aria-hidden", "true");
const still = matchMedia("(prefers-reduced-motion: reduce)").matches;
let at = 0;
let timer;
function snapHome() {
  track.classList.add("snap");
  at = 0;
  track.style.transform = "translateX(0%)";
  track.offsetWidth;
  track.classList.remove("snap");
}
function go(i) {
  // Sitting on the copy of the first quote: jump back to the real one unseen first.
  if (at >= slides.length) {
    snapHome();
    if (i >= slides.length) i -= slides.length;
  }
  at = Math.min(i, slides.length);
  track.style.transform = `translateX(${-100 * at}%)`;
  dots.forEach((d, j) => d.setAttribute("aria-current", String(j === at % slides.length)));
  restart();
}
track.addEventListener("transitionend", () => at >= slides.length && snapHome());
function restart() {
  clearTimeout(timer);
  if (!still) timer = setTimeout(() => !document.hidden && go(at + 1), 5500);
}
track.addEventListener("click", () => go(still ? (at + 1) % slides.length : at + 1));
dots.forEach((d, i) => d.addEventListener("click", () => go(i)));
const carousel = document.querySelector(".carousel");
carousel.addEventListener("mouseenter", () => clearTimeout(timer));
carousel.addEventListener("mouseleave", restart);
document.addEventListener("visibilitychange", restart);
go(0);

// Soft entrance on scroll; a quiet border on the top bar once you've moved.
const io = new IntersectionObserver(
  (entries) => entries.forEach((e) => e.isIntersecting && (e.target.classList.add("in"), io.unobserve(e.target))),
  { rootMargin: "0px 0px -8% 0px" },
);
document.querySelectorAll(".reveal").forEach((el) => io.observe(el));
const bar = document.querySelector(".top");
addEventListener("scroll", () => bar.classList.toggle("scrolled", scrollY > 8), { passive: true });
