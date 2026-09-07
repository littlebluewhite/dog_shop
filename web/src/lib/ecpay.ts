import type { EcpayForm } from '$lib/types';

/** 用隱藏表單把欄位 POST 到綠界（頂層導頁，不用 iframe；規格 §7 第 7 點）。只能在瀏覽器呼叫 */
export function postToEcpay(form: EcpayForm): void {
	const el = document.createElement('form');
	el.method = 'POST';
	el.action = form.action;
	el.style.display = 'none';
	for (const [name, value] of Object.entries(form.fields)) {
		const input = document.createElement('input');
		input.type = 'hidden';
		input.name = name;
		input.value = value;
		el.appendChild(input);
	}
	document.body.appendChild(el);
	el.submit();
}
