import "../../../chunks/server2.js";
import "../../../chunks/index-server2.js";
import "../../../chunks/state.js";
import { t as afterNavigate } from "../../../chunks/navigation.js";
import { n as installDirtyGuard } from "../../../chunks/entity-navigation.js";
import { t as createSupabaseClient } from "../../../chunks/supabase.js";
import "../../../chunks/button.js";
import "../../../chunks/runtime.js";
function Provider(_props) {}
// src/routes/(app)/+layout.svelte
function _layout($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		afterNavigate(() => document.body.classList.add("hydrated"));
		let { children } = $$props;
		installDirtyGuard();
		createSupabaseClient();
		const domains = [
			{
				name: "common",
				label: "Common",
				tier: "extended",
				entities: [
					{
						path: "/common/code",
						label: "Code"
					},
					{
						path: "/common/currency-code-list",
						label: "Currency Code List"
					},
					{
						path: "/common/date",
						label: "Date"
					},
					{
						path: "/common/distribution-base",
						label: "Distribution Base"
					},
					{
						path: "/common/effective-date",
						label: "Effective Date"
					},
					{
						path: "/common/event-base",
						label: "Event Base"
					},
					{
						path: "/common/formatted-date-time",
						label: "Formatted Date Time"
					},
					{
						path: "/common/gender-code-list",
						label: "Gender Code List"
					},
					{
						path: "/common/identifier",
						label: "Identifier"
					},
					{
						path: "/common/name",
						label: "Name"
					},
					{
						path: "/common/person-base",
						label: "Person Base"
					},
					{
						path: "/common/position-schedule-type-code-list",
						label: "Position Schedule Type Code List"
					},
					{
						path: "/common/string-type-array",
						label: "String Type Array"
					},
					{
						path: "/common/amount",
						label: "Amount"
					},
					{
						path: "/common/process-history-item",
						label: "Process History Item"
					},
					{
						path: "/common/process-history",
						label: "Process History"
					}
				]
			},
			{
				name: "compensation",
				label: "Compensation",
				tier: "extended",
				entities: [{
					path: "/compensation/pay-runs",
					label: "Pay Run"
				}]
			},
			{
				name: "events",
				label: "Events",
				tier: "extended",
				entities: [{
					path: "/events/public-event",
					label: "Public Event"
				}]
			},
			{
				name: "recruiting",
				label: "Recruiting",
				tier: "extended",
				entities: [
					{
						path: "/recruiting/grant-application",
						label: "Grant Application"
					},
					{
						path: "/recruiting/grant-audit-trail",
						label: "Grant Audit Trail"
					},
					{
						path: "/recruiting/grant-panel-minute",
						label: "Grant Panel Minute"
					},
					{
						path: "/recruiting/grant-review-note",
						label: "Grant Review Note"
					},
					{
						path: "/recruiting/applications",
						label: "Application"
					},
					{
						path: "/recruiting/candidate",
						label: "Candidate"
					}
				]
			},
			{
				name: "rsvp",
				label: "RSVP",
				tier: "extended",
				entities: [{
					path: "/rsvp/rsvp",
					label: "Rsvp"
				}]
			}
		];
		domains.filter((d) => d.tier === "core");
		domains.filter((d) => d.tier !== "core");
		if (Provider) {
			$$renderer.push("<!--[-->");
			$$renderer.push("<!--]-->");
		} else {
			$$renderer.push("<!--[!-->");
			$$renderer.push("<!--]-->");
		}
		$$renderer.push(` `);
		$$renderer.push(`<!---->`);
	});
}
export { _layout as default };

//# sourceMappingURL=_layout.svelte.js.map