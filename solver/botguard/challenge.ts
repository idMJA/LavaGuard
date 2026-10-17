import {
	buildURL,
	INNERTUBE_API_KEY,
	parse_json,
	REQUEST_KEY,
	TV_CONFIG,
	TV_USER_AGENT,
	toUint8,
	USER_AGENT,
	utf8Decode,
	YT_BASE,
} from "./helpers.ts";

export interface BotGuardChallenge {
	program: string;
	globalName: string;
	interpreterUrl?: string;
	interpreterJavascript?: string;
}

export function parseWaaChallenge(rawData: unknown): BotGuardChallenge | null {
	const rawObj = rawData as Record<string, unknown>;
	if (rawObj?.bgChallenge) {
		const bg = rawObj.bgChallenge as Record<string, unknown>;
		return {
			program: bg.program as string,
			globalName: bg.globalName as string,
			interpreterUrl: (bg.interpreterUrl as Record<string, string>)
				?.privateDoNotAccessOrElseTrustedResourceUrlWrappedValue,
			interpreterJavascript: (
				bg.interpreterJavascript as Record<string, string>
			)?.privateDoNotAccessOrElseSafeScriptWrappedValue,
		};
	}

	const rawArr = rawData as unknown[];
	let challengeData: unknown[];

	if (rawArr?.length > 1 && typeof rawArr[1] === "string") {
		const bytes = toUint8(rawArr[1]);
		challengeData = JSON.parse(utf8Decode(bytes.map((value) => value + 97)));
	} else if (Array.isArray(rawArr?.[0])) {
		challengeData = rawArr[0];
	} else if (Array.isArray(rawData)) {
		challengeData = rawData;
	} else {
		return null;
	}

	if (!Array.isArray(challengeData)) return null;

	const [, wrappedScript, wrappedUrl, , program, globalName] = challengeData;
	const script = Array.isArray(wrappedScript)
		? wrappedScript.find((value) => typeof value === "string")
		: undefined;
	const interpreterUrl = Array.isArray(wrappedUrl)
		? wrappedUrl.find((value) => typeof value === "string")
		: undefined;

	if (!program || !globalName || (!interpreterUrl && !script)) return null;

	return {
		program: program as string,
		globalName: globalName as string,
		interpreterUrl: interpreterUrl as string | undefined,
		interpreterJavascript: script as string | undefined,
	};
}

export async function fetchChallenge(): Promise<{
	challenge: BotGuardChallenge;
	key: string;
}> {
	let key = REQUEST_KEY;
	let challenge: BotGuardChallenge | null = null;
	let eacrToken: string | undefined;

	try {
		const res = await fetch(YT_BASE, {
			headers: {
				accept: "*/*",
				"accept-language": "en-US,en;q=0.7",
				"user-agent": USER_AGENT,
			},
		});
		const txt = await res.text();

		const attestation = txt.match(/window\.ytAtN\(\s*({[\s\S]*?})\s*\)/);
		if (attestation) {
			try {
				const parsed = parse_json(attestation[1]) as Record<string, unknown>;
				if (parsed && typeof parsed === "object") {
					challenge = (parsed as { bgChallenge?: unknown }).bgChallenge
						? (parsed as unknown as BotGuardChallenge)
						: parseWaaChallenge((parsed as { R?: unknown }).R);
					eacrToken = (parsed as { T?: string }).T;
				}
			} catch {}
		}

		if (!challenge && !eacrToken) {
			const initialAttestationMatch = txt.match(
				/initialAttestationDataJson\s*=\s*({[\s\S]*?});/,
			);
			if (initialAttestationMatch) {
				try {
					const parsed = parse_json(initialAttestationMatch[1]) as Record<
						string,
						unknown
					>;
					if (parsed && typeof parsed === "object") {
						challenge = (parsed as { bgChallenge?: unknown }).bgChallenge
							? (parsed as unknown as BotGuardChallenge)
							: parseWaaChallenge((parsed as { R?: unknown }).R);
						eacrToken = (parsed as { T?: string }).T || eacrToken;
					}
				} catch {}
			}
		}
	} catch {}

	if (!challenge) {
		try {
			const payload: Record<string, unknown> = {
				context: {
					client: {
						clientName: "WEB",
						clientVersion: "2.20260227.01.00",
					},
				},
				engagementType: "ENGAGEMENT_TYPE_UNBOUND",
			};
			if (eacrToken) payload.eacrToken = eacrToken;

			const attRes = await fetch(
				`${YT_BASE}/youtubei/v1/att/get?prettyPrint=false`,
				{
					method: "POST",
					headers: {
						accept: "*/*",
						"content-type": "application/json",
						"user-agent": USER_AGENT,
						"x-goog-api-key": INNERTUBE_API_KEY,
					},
					body: JSON.stringify(payload),
				},
			);

			if (attRes.ok) {
				const attestation = (await attRes.json()) as Record<string, unknown>;
				if (attestation?.bgChallenge) {
					challenge = parseWaaChallenge(attestation);
				}
			}
		} catch {}
	}

	if (!challenge) {
		try {
			const waaRes = await fetch(buildURL("Create", false), {
				method: "POST",
				headers: {
					"content-type": "application/json+protobuf",
					"x-goog-api-key": INNERTUBE_API_KEY,
					"x-user-agent": "grpc-web-javascript/0.1",
					"user-agent": USER_AGENT,
				},
				body: JSON.stringify([key]),
			});

			if (waaRes.ok) {
				challenge = parseWaaChallenge(await waaRes.json());
			}
		} catch {}
	}

	if (!challenge) {
		try {
			const tvRes = await fetch(TV_CONFIG, {
				headers: { accept: "*/*", "user-agent": TV_USER_AGENT },
			});
			const txt = await tvRes.text();
			if (txt.startsWith(")]}'")) {
				const json = JSON.parse(txt.slice(4));
				challenge = json.challengeParams?.R
					? (parseWaaChallenge(JSON.parse(json.challengeParams.R)) ??
						parseWaaChallenge(json.challengeParams.R))
					: null;
				key = json.challengeRequestKey || key;
			}
		} catch {}
	}

	if (!challenge) {
		throw new Error("failed obtaining botguard challenge from all fallbacks");
	}

	return { challenge, key };
}
