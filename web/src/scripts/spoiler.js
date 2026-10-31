document.addEventListener("click", (e) => {
  const spoiler = e.target.closest?.(".spoiler");
  if (spoiler) spoiler.classList.add("revealed");
});
