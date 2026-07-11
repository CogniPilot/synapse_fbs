(function () {
  const versions = [
    { dir: "main", label: "main (development)" },
    { dir: "0.6", label: "0.6" },
    { dir: "0.5", label: "0.5" },
    { dir: "0.4", label: "0.4" },
    { dir: "0.3", label: "0.3" },
  ];
  const current = "main";
  function docsBaseUrl() {
    const script = document.currentScript || document.querySelector('script[src*="version-selector"]');
    if (!script) {
      return new URL('../', window.location.href);
    }
    const scriptUrl = new URL(script.getAttribute('src'), window.location.href);
    return new URL('../../', scriptUrl);
  }

  function targetUrl(dir) {
    return new URL(dir.replace(/\/+$/, '') + '/', docsBaseUrl()).href;
  }

  function buildSelect() {
    const select = document.createElement('select');
    select.className = 'synapse-version-select';
    select.setAttribute('aria-label', 'Schema documentation version');
    for (const version of versions) {
      const option = document.createElement('option');
      option.value = version.dir;
      option.textContent = version.label;
      option.selected = version.dir === current;
      select.appendChild(option);
    }
    select.addEventListener('change', () => {
      window.location.href = targetUrl(select.value);
    });
    return select;
  }

  function mountMenu() {
    const menu = document.getElementById('mdbook-menu-bar');
    if (!menu || menu.querySelector('.synapse-version-menu')) {
      return;
    }
    const target = menu.querySelector('.right-buttons') || menu;
    const wrapper = document.createElement('div');
    wrapper.className = 'synapse-version-menu';
    const label = document.createElement('label');
    label.textContent = 'Docs';
    wrapper.appendChild(label);
    wrapper.appendChild(buildSelect());
    target.insertBefore(wrapper, target.firstChild);
  }

  function mount() {
    mountMenu();
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', mount);
  } else {
    mount();
  }
})();
