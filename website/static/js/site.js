// Wurzelwerk website – parallax, root growth, scroll reveal. No dependencies.
//
//   [data-speed]  hero layers: translateY(scrollY * speed) while the hero is on screen
//   [data-fade]   fades out over the first viewport of scrolling
//   [data-depth]  inside a [data-scope] section: offset by how far the section is from the
//                 viewport centre, plus a mouse offset inside [data-tilt]
//   [data-grow]   sets --grow (0…1) as the section scrolls through the viewport (roots)
(() => {
  document.documentElement.classList.add("js");

  const reduce = window.matchMedia("(prefers-reduced-motion: reduce)");
  const nav = document.querySelector("[data-nav]");
  const hero = document.querySelector("[data-hero]");
  const speedEls = [...document.querySelectorAll("[data-speed]")];
  const fadeEls = [...document.querySelectorAll("[data-fade]")];
  const growEls = [...document.querySelectorAll("[data-grow]")];
  const scopes = [...document.querySelectorAll("[data-scope]")].map((scope) => ({
    scope,
    layers: [...scope.querySelectorAll("[data-depth]")].map((el) => ({ el, depth: parseFloat(el.dataset.depth) })),
  }));
  const mouse = { x: 0, y: 0 };

  let queued = false;
  const queue = () => {
    if (!queued) {
      queued = true;
      requestAnimationFrame(update);
    }
  };

  function update() {
    queued = false;
    const y = window.scrollY;
    const vh = window.innerHeight;
    nav?.classList.toggle("is-scrolled", y > 40);

    if (reduce.matches) return;

    // Hero: only while visible, so off-screen layers stop costing paint time.
    if (hero && y < hero.offsetHeight) {
      for (const el of speedEls) el.style.transform = `translate3d(0, ${(y * el.dataset.speed).toFixed(1)}px, 0)`;
      const fade = Math.max(0, 1 - y / (vh * 0.7));
      for (const el of fadeEls) el.style.opacity = fade.toFixed(3);
    }

    for (const el of growEls) {
      const r = el.getBoundingClientRect();
      // 0 when the section top enters the viewport bottom, 1 when its bottom reaches ~70 % of the viewport.
      const p = (vh - r.top) / (r.height + vh * 0.3);
      el.style.setProperty("--grow", Math.min(1, Math.max(0, p)).toFixed(3));
    }

    for (const { scope, layers } of scopes) {
      const r = scope.getBoundingClientRect();
      if (r.bottom < -200 || r.top > vh + 200) continue;
      const offset = r.top + r.height / 2 - vh / 2;
      for (const { el, depth } of layers) {
        const dx = mouse.x * depth * 60;
        const dy = -offset * depth + mouse.y * depth * 60;
        el.style.transform = `translate3d(${dx.toFixed(1)}px, ${dy.toFixed(1)}px, 0)`;
      }
    }
  }

  // Mouse parallax on the board stage: each layer shifts by its depth, the board tilts slightly.
  for (const stage of document.querySelectorAll("[data-tilt]")) {
    const board = stage.querySelector(".stage__board");
    stage.addEventListener("pointermove", (e) => {
      if (reduce.matches || e.pointerType !== "mouse") return;
      const r = stage.getBoundingClientRect();
      mouse.x = (e.clientX - r.left) / r.width - 0.5;
      mouse.y = (e.clientY - r.top) / r.height - 0.5;
      board.style.rotate = `${(-mouse.y).toFixed(3)} ${mouse.x.toFixed(3)} 0 ${(Math.hypot(mouse.x, mouse.y) * 5).toFixed(2)}deg`;
      queue();
    });
    stage.addEventListener("pointerleave", () => {
      mouse.x = mouse.y = 0;
      board.style.rotate = "";
      queue();
    });
    board.style.transition = "rotate .4s ease-out";
  }

  // Reveal on first sight.
  const io = new IntersectionObserver(
    (entries) => {
      for (const e of entries) {
        if (e.isIntersecting) {
          e.target.classList.add("is-visible");
          io.unobserve(e.target);
        }
      }
    },
    { rootMargin: "0px 0px -10% 0px" },
  );
  document.querySelectorAll(".reveal").forEach((el) => io.observe(el));

  reduce.addEventListener("change", () => {
    for (const el of document.querySelectorAll("[data-speed], [data-depth]")) el.style.transform = "";
    for (const el of fadeEls) el.style.opacity = "";
    queue();
  });
  window.addEventListener("scroll", queue, { passive: true });
  window.addEventListener("resize", queue);
  queue();
})();
