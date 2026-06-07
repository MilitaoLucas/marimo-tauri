(function() {
    var HOME = 'http://localhost:2730/';
    function onHomePage() {
        var h = window.location.href;
        return h === HOME || h === HOME.slice(0,-1) || h === 'http://localhost:2730';
    }

    var tbHidden = localStorage.getItem('__tb_hidden__') === '1';

    // Push marimo's UI below the title bar. Marimo's root uses
    // position:fixed, so margins/padding don't move it. Apply a transform
    // to <body> so it becomes the containing block for fixed descendants,
    // and inset/size body to leave 36px at the top (or 0 when bar is hidden).
    var st = document.createElement('style');
    st.id = '__tb_st__';
    function updateBodyStyle() {
        var top = tbHidden ? '0px' : '36px';
        var h   = tbHidden ? '100vh' : 'calc(100vh - 36px)';
        st.textContent = ''
            + 'html,body{margin:0!important;padding:0!important}'
            + 'body{position:absolute!important;top:'+top+'!important;left:0!important;right:0!important;bottom:0!important;'
            +       'height:'+h+'!important;width:100vw!important;overflow:'+(onHomePage()?'auto':'hidden')+'!important;'
            +       'transform:translateZ(0)!important}';
    }
    updateBodyStyle();
    (document.head || document.documentElement).appendChild(st);

    // Marimo opens notebooks via <a target="_blank">, which Tauri routes
    // to on_new_window. We want plain clicks to navigate in-place (so the
    // user stays in the main window), and only modifier/middle clicks to
    // create a new window. Intercept plain left-clicks here; let modified
    // clicks fall through to Tauri's on_new_window handler.
    document.addEventListener('click', function(e) {
        if (e.button !== 0) return;
        if (e.ctrlKey || e.metaKey || e.shiftKey || e.altKey) return;
        if (e.defaultPrevented) return;
        var a = e.target.closest && e.target.closest('a[href]');
        if (!a) return;
        e.preventDefault();
        window.location.href = a.href;
    }, true);

    function toggleTitleBar() {
        tbHidden = !tbHidden;
        localStorage.setItem('__tb_hidden__', tbHidden ? '1' : '0');
        updateBodyStyle();
        var bar = document.getElementById('__tb__');
        if (bar) bar.style.display = tbHidden ? 'none' : '';
    }

    // Ctrl+Shift+B toggles the title bar
    document.addEventListener('keydown', function(e) {
        if (e.ctrlKey && e.shiftKey && (e.key === 'B' || e.key === 'b')) {
            toggleTitleBar();
        }
    });

    function showContextMenu(x, y) {
        var existing = document.getElementById('__tb_ctx__');
        if (existing) existing.remove();

        var isDark = window.matchMedia('(prefers-color-scheme:dark)').matches;
        var bg     = isDark ? '#2d2d2d' : '#ffffff';
        var fg     = isDark ? '#cccccc' : '#333333';
        var border = isDark ? '#444444' : '#cccccc';

        var menu = document.createElement('div');
        menu.id = '__tb_ctx__';
        menu.style.cssText = [
            'position:fixed','z-index:2147483648',
            'left:'+x+'px','top:'+y+'px',
            'background:'+bg,'border:1px solid '+border,
            'border-radius:6px','padding:4px 0',
            'box-shadow:0 4px 12px rgba(0,0,0,0.3)',
            'font-family:system-ui,sans-serif','font-size:13px',
            'min-width:160px',
        ].join(';');

        var item = document.createElement('div');
        item.textContent = 'Hide title bar';
        item.style.cssText = 'padding:6px 16px;cursor:pointer;color:'+fg+';';
        item.onmouseover = function() { this.style.background=isDark?'rgba(255,255,255,0.1)':'rgba(0,0,0,0.07)'; };
        item.onmouseout  = function() { this.style.background='transparent'; };
        item.onclick = function() { menu.remove(); toggleTitleBar(); };
        menu.appendChild(item);

        document.documentElement.appendChild(menu);
        // Dismiss on next click anywhere
        setTimeout(function() {
            document.addEventListener('click', function dismiss() {
                menu.remove();
                document.removeEventListener('click', dismiss);
            }, { once: true });
        }, 0);
    }

    function buildBar() {
        if (document.getElementById('__tb__')) return;
        var isDark = window.matchMedia('(prefers-color-scheme:dark)').matches;
        var bg   = isDark ? '#1e1e1e' : '#f3f3f3';
        var fg   = isDark ? '#cccccc' : '#333333';
        var sep  = isDark ? 'rgba(255,255,255,0.08)' : 'rgba(0,0,0,0.1)';

        var bar = document.createElement('div');
        bar.id  = '__tb__';
        bar.setAttribute('data-tauri-drag-region','');
        bar.style.cssText = [
            'position:fixed','top:0','left:0','right:0','height:36px',
            'z-index:2147483647','display:flex','align-items:center',
            'background:'+bg,'border-bottom:1px solid '+sep,
            'user-select:none','-webkit-user-select:none',
        ].join(';');
        if (tbHidden) bar.style.display = 'none';

        bar.addEventListener('contextmenu', function(e) {
            e.preventDefault();
            showContextMenu(e.clientX, e.clientY);
        });

        // ← Home button (hidden on home page)
        if (!onHomePage()) {
            var home = document.createElement('button');
            home.textContent = '← Home';
            home.style.cssText = [
                'margin-left:8px','padding:3px 10px','border-radius:5px',
                'border:none','background:transparent','cursor:pointer',
                'font-size:12px','font-family:system-ui,sans-serif',
                'color:'+fg,'pointer-events:all','flex-shrink:0',
            ].join(';');
            home.onmouseover = function() { this.style.background=isDark?'rgba(255,255,255,0.1)':'rgba(0,0,0,0.07)'; };
            home.onmouseout  = function() { this.style.background='transparent'; };
            home.onclick = function(e) { e.stopPropagation(); window.location.href=HOME; };
            bar.appendChild(home);
        }

        // Drag spacer
        var drag = document.createElement('div');
        drag.setAttribute('data-tauri-drag-region','');
        drag.style.cssText = 'flex:1;height:100%;';
        bar.appendChild(drag);

        // Window controls
        [
            { sym:'−', tip:'Minimize', fn:function() { window.__TAURI__.window.getCurrentWindow().minimize(); } },
            { sym:'×', tip:'Close',    fn:function() { window.__TAURI__.window.getCurrentWindow().close(); } },
        ].forEach(function(c) {
            var b = document.createElement('button');
            b.textContent = c.sym;
            b.title = c.tip;
            b.style.cssText = [
                'width:46px','height:36px','border:none','background:transparent',
                'cursor:pointer','font-size:14px','display:flex','align-items:center',
                'justify-content:center','pointer-events:all','color:'+fg,'flex-shrink:0',
            ].join(';');
            var isClose = c.tip === 'Close';
            b.onmouseover = function() { this.style.background = isClose ? '#c42b1c' : (isDark?'rgba(255,255,255,0.1)':'rgba(0,0,0,0.07)'); if(isClose) this.style.color='#fff'; };
            b.onmouseout  = function() { this.style.background='transparent'; this.style.color=fg; };
            b.onclick = function(e) { e.stopPropagation(); c.fn(); };
            bar.appendChild(b);
        });

        // Attach to <html> so the bar is NOT inside the transformed body
        // (otherwise the transform would also offset the bar itself).
        document.documentElement.appendChild(bar);
    }

    // documentElement always exists when the init script runs, so we can
    // build the bar synchronously — this avoids a flicker on navigation
    // where the new page would otherwise paint once before DOMContentLoaded.
    buildBar();
})();
