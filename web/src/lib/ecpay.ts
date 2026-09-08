import type { EcpayForm } from '$lib/types';

/** 用隱藏表單把欄位 POST 到綠界（規格 §7 第 7 點）。target='_blank' 開新分頁（列印託運單；綠界禁止 iframe） */
export function postToEcpay(form: EcpayForm, target?: '_blank'): void {
	const el = document.createElement('form');
	el.method = 'POST';
	el.action = form.action;
	if (target) el.target = target;
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
	if (target) el.remove();
}
