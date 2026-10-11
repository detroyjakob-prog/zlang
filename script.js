/* ============================================================
   Z Programming Language — interactions
   Terminal typewriter, tabs, scroll reveal, nav, copy button.
   ============================================================ */
(function () {
  "use strict";

  /* ---------- Sticky nav shadow ---------- */
  var nav = document.getElementById("nav");
  function onScroll() {
    if (window.scrollY > 8) nav.classList.add("scrolled");
    else nav.classList.remove("scrolled");
  }
  window.addEventListener("scroll", onScroll, { passive: true });
  onScroll();

  /* ---------- Mobile menu ---------- */
  var toggle = document.getElementById("menuToggle");
  var menu = document.getElementById("mobileMenu");
  toggle.addEventListener("click", function () {
    menu.classList.toggle("open");
  });
  menu.querySelectorAll("a").forEach(function (a) {
    a.addEventListener("click", function () {
      menu.classList.remove("open");
    });
  });

  /* ---------- Copy install command ---------- */
  var copyBtn = document.getElementById("copyBtn");
  var installCmd = document.getElementById("installCmd");
  copyBtn.addEventListener("click", function () {
    var text = installCmd.textContent;
    var done = function () {
      copyBtn.classList.add("ok");
      setTimeout(function () { copyBtn.classList.remove("ok"); }, 1400);
    };
    if (navigator.clipboard && navigator.clipboard.writeText) {
      navigator.clipboard.writeText(text).then(done).catch(done);
    } else {
      var ta = document.createElement("textarea");
      ta.value = text;
      document.body.appendChild(ta);
      ta.select();
      try { document.execCommand("copy"); } catch (e) {}
      document.body.removeChild(ta);
      done();
    }
  });

  /* ---------- Tabs ---------- */
  var tabBtns = document.querySelectorAll(".tab-btn");
  tabBtns.forEach(function (btn) {
    btn.addEventListener("click", function () {
      var id = btn.getAttribute("data-tab");
      tabBtns.forEach(function (b) { b.classList.remove("active"); });
      document.querySelectorAll(".tab-panel").forEach(function (p) {
        p.classList.remove("active");
      });
      btn.classList.add("active");
      var panel = document.getElementById(id);
      if (panel) panel.classList.add("active");
    });
  });

  /* ---------- Scroll reveal ---------- */
  var revealEls = document.querySelectorAll(".reveal");
  if ("IntersectionObserver" in window) {
    var io = new IntersectionObserver(function (entries) {
      entries.forEach(function (entry) {
        if (entry.isIntersecting) {
          entry.target.classList.add("in");
          io.unobserve(entry.target);
        }
      });
    }, { threshold: 0.12, rootMargin: "0px 0px -40px 0px" });
    revealEls.forEach(function (el) { io.observe(el); });
  } else {
    revealEls.forEach(function (el) { el.classList.add("in"); });
  }

  /* ---------- Animated terminal ----------
     Each line has:
       html  — the final highlighted markup
       type  — true to type char-by-char (commands), false to print (output)
       pause — delay after the line finishes
  */
  var termBody = document.getElementById("termBody");
  var script = [
    { html: '<span class="p">z❯</span> <span class="cmd">z run hello.z</span>', type: true, pause: 500 },
    { html: '<span class="kw">fn</span> <span class="fn">Main</span>() {', type: true, pause: 120 },
    { html: '    <span class="kw">call</span> <span class="fn">printer</span>(<span class="str">"Hello, Z!"</span>)', type: true, pause: 120 },
    { html: '}', type: true, pause: 260 },
    { html: '<span class="out">Hello, Z!</span>', type: false, pause: 900 },
    { html: '<span class="p">z❯</span> <span class="cmd">abl squares = [x * x for x in range(6)]</span>', type: true, pause: 500 },
    { html: '<span class="out">[0, 1, 4, 9, 16, 25]</span>', type: false, pause: 1800 }
  ];

  function plainText(html) {
    var d = document.createElement("div");
    d.innerHTML = html;
    return d.textContent;
  }

  function addLine(html) {
    var line = document.createElement("div");
    line.className = "term-line";
    line.innerHTML = html;
    termBody.appendChild(line);
    return line;
  }

  function typeLine(step, done) {
    var line = addLine('<span class="cursor"></span>');
    var cursor = line.firstChild;
    var target = plainText(step.html);
    var i = 0;
    (function tick() {
      i += 1;
      cursor.insertAdjacentText("beforebegin", target.charAt(i - 1));
      if (i < target.length) {
        setTimeout(tick, 24);
      } else {
        line.innerHTML = step.html;   // swap in highlighted markup
        done();
      }
    })();
  }

  function play() {
    var idx = 0;
    (function next() {
      if (idx >= script.length) {
        setTimeout(function () {
          termBody.innerHTML = "";
          play();
        }, 3200);
        return;
      }
      var step = script[idx];
      idx += 1;
      if (step.type) {
        typeLine(step, function () { setTimeout(next, step.pause); });
      } else {
        var line = addLine(step.html);
        line.style.opacity = "0";
        requestAnimationFrame(function () {
          line.style.transition = "opacity .28s ease";
          line.style.opacity = "1";
        });
        setTimeout(next, step.pause);
      }
    })();
  }

  if (window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
    script.forEach(function (s) { addLine(s.html); });
  } else {
    play();
  }
})();

