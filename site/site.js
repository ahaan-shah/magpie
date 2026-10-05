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

// Soft entrance on scroll; a quiet border on the top bar once you've moved.
const io = new IntersectionObserver(
  (entries) => entries.forEach((e) => e.isIntersecting && (e.target.classList.add("in"), io.unobserve(e.target))),
  { rootMargin: "0px 0px -8% 0px" },
);
document.querySelectorAll(".reveal").forEach((el) => io.observe(el));
const bar = document.querySelector(".top");
addEventListener("scroll", () => bar.classList.toggle("scrolled", scrollY > 8), { passive: true });
