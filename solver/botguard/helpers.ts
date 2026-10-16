export const GOOGLE_API_BASE = "https://jnn-pa.googleapis.com";
export const YT_BASE = "https://www.youtube.com";
export const INNERTUBE_API_KEY = "AIzaSyDyT5W0Jh49F30Pqqtyfdf7pDLFKLJoAnw";
export const USER_AGENT =
	"Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
export const TV_USER_AGENT =
	"Mozilla/5.0 (PlayStation; PlayStation 4/12.00) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/12.00 Safari/605.1.15";

export const REQUEST_KEY = "O43z0dpjhgX20SCx4KAo";
export const TV_CONFIG =
	"https://www.youtube.com/tv_config?action_get_config=true&client=lb4&theme=cl";

export const BASE64_MAP: Record<string, string> = {
	"-": "+",
	_: "/",
	".": "=",
};
export const REG_FOR_BASE64 = /[-_.]/g;

export function toUint8(base64: string): Uint8Array {
	const base64Mod = base64.replace(
		REG_FOR_BASE64,
		(match) => BASE64_MAP[match] || match,
	);
	const binary = atob(base64Mod);
	const bytes = new Uint8Array(binary.length);
	for (let i = 0; i < binary.length; i++) {
		bytes[i] = binary.charCodeAt(i);
	}
	return bytes;
}

export function toBase64(u8: Uint8Array, base64url = false): string {
	let binary = "";
	for (let i = 0; i < u8.length; i++) {
		const byte = u8[i];
		if (byte !== undefined) {
			binary += String.fromCharCode(byte);
		}
	}
	const base64 = btoa(binary);
	if (base64url) {
		return base64.replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
	}
	return base64;
}

export function buildURL(endpoint: string, use_api = true): string {
	return `${use_api ? YT_BASE : GOOGLE_API_BASE}/${use_api ? "api/jnn/v1" : "$rpc/google.internal.waa.v1.Waa"}/${endpoint}`;
}

export function parse_json(looseJson: string): unknown {
	const jsonStr = looseJson
		.replace(/,\s*([\]}])/g, "$1")
		.replace(/\\x([0-9A-Fa-f]{2})/g, "\\u00$1")
		.replace(/'((?:[^'\\]|\\[\s\S])*)'/g, (_match, innerStr) => {
			return `"${innerStr.replace(/\\'/g, "'").replace(/"/g, '\\"')}"`;
		});

	try {
		return JSON.parse(jsonStr);
	} catch {
		const reg = jsonStr.replace(/([{,]\s*)([a-zA-Z0-9_$]+)\s*:/g, '$1"$2":');
		return JSON.parse(reg);
	}
}

export function utf8Encode(str: string): Uint8Array {
	const arr = new Uint8Array(str.length);
	for (let i = 0; i < str.length; i++) {
		arr[i] = str.charCodeAt(i) & 0xff;
	}
	return arr;
}

export function utf8Decode(bytes: Uint8Array): string {
	let s = "";
	for (let i = 0; i < bytes.length; i++) {
		const b = bytes[i];
		if (b !== undefined) s += String.fromCharCode(b);
	}
	return s;
}

export function decodeColdStartToken(token: string) {
	const packet = toUint8(token);

	if (packet.length < 10 || packet[0] !== 34) {
		throw new Error("invalid cold start token");
	}

	const length = packet[1];
	if (length === undefined || packet.length !== length + 2 || length < 8) {
		throw new Error("invalid cold start packet length");
	}

	const payload = packet.slice(2);
	for (let i = 2; i < payload.length; i++) {
		const k = payload[i % 2];
		const curr = payload[i];
		if (k !== undefined && curr !== undefined) {
			payload[i] = curr ^ k;
		}
	}

	const b4 = payload[4] ?? 0;
	const b5 = payload[5] ?? 0;
	const b6 = payload[6] ?? 0;
	const b7 = payload[7] ?? 0;
	const timestamp = ((b4 << 24) | (b5 << 16) | (b6 << 8) | b7) >>> 0;

	return {
		contentBinding: utf8Decode(payload.subarray(8)),
		timestamp,
		unknownVal: payload[2],
		clientState: payload[3],
		keys: [payload[0], payload[1]],
		date: new Date(timestamp * 1000).toISOString(),
	};
}

export function createColdStartToken(contentBinding: string, clientState = 1) {
	const bytecode = utf8Encode(contentBinding);
	if (bytecode.length > 247) return null;

	const timestamp = Math.floor(Date.now() / 1000);
	const randKeys = [
		Math.floor(Math.random() * 256),
		Math.floor(Math.random() * 256),
	];

	const header = randKeys.concat(
		[0, clientState],
		[
			(timestamp >> 24) & 0xff,
			(timestamp >> 16) & 0xff,
			(timestamp >> 8) & 0xff,
			timestamp & 0xff,
		],
	);

	const packet = new Uint8Array(2 + header.length + bytecode.length);
	packet[0] = 34;
	packet[1] = header.length + bytecode.length;
	packet.set(header, 2);
	packet.set(bytecode, 2 + header.length);

	const payload = packet.subarray(2);
	for (let i = 2; i < payload.length; i++) {
		const k = payload[i % 2];
		const curr = payload[i];
		if (k !== undefined && curr !== undefined) {
			payload[i] = curr ^ k;
		}
	}

	return toBase64(packet, true);
}
