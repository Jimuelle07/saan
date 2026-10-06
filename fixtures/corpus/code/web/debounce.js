/**
 * Debounce and throttle helpers for search-as-you-type inputs and scroll handlers.
 */

export function debounce(fn, waitMs = 250, { leading = false } = {}) {
  let timer = null;
  return function debounced(...args) {
    const callNow = leading && timer === null;
    clearTimeout(timer);
    timer = setTimeout(() => {
      timer = null;
      if (!leading) fn.apply(this, args);
    }, waitMs);
    if (callNow) fn.apply(this, args);
  };
}

export function throttle(fn, intervalMs = 100) {
  let last = 0;
  let pending = null;
  return function throttled(...args) {
    const now = Date.now();
    const remaining = intervalMs - (now - last);
    if (remaining <= 0) {
      last = now;
      fn.apply(this, args);
    } else if (!pending) {
      pending = setTimeout(() => {
        pending = null;
        last = Date.now();
        fn.apply(this, args);
      }, remaining);
    }
  };
}

// Example: only hit the search API after the user stops typing for 300ms.
const input = document.querySelector("#search");
if (input) {
  input.addEventListener(
    "input",
    debounce(async (event) => {
      const res = await fetch(`/api/search?q=${encodeURIComponent(event.target.value)}`);
      console.log(await res.json());
    }, 300),
  );
}
