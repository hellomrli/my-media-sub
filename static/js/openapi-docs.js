fetch('/openapi.json', {credentials: 'same-origin'}).then(response => response.json()).then(spec => {
  const root = document.getElementById('paths');
  Object.entries(spec.paths || {}).forEach(([path, methods]) => {
    const section = document.createElement('section'); section.className = 'app-panel-soft p-3';
    const title = document.createElement('h2'); title.className = 'font-mono text-primary'; title.textContent = path; section.appendChild(title);
    Object.entries(methods).forEach(([method, operation]) => { const row = document.createElement('p'); row.className = 'text-sm mt-2'; row.textContent = `${method.toUpperCase()} — ${operation.summary || ''}`; section.appendChild(row); });
    root.appendChild(section);
  });
}).catch(error => { document.getElementById('paths').textContent = `加载失败：${error.message}`; });
