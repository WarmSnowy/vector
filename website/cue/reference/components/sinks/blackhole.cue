package metadata

components: sinks: blackhole: {
	title: "Blackhole"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		development:   "stable"
		egress_method: "stream"
		service_providers: []
		stateful: false
	}

	features: {
		acknowledgements: false
		healthcheck: enabled: false
		send: {
			compression: enabled: false
			encoding: enabled:    false
			request: enabled:     false
			tls: enabled:         false
		}
	}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	configuration: base.components.sinks.blackhole.configuration

	input: {
		logs: true
		metrics: {
			counter:      true
			distribution: true
			gauge:        true
			histogram:    true
			set:          true
			summary:      true
		}
		traces: true
	}

	telemetry: metrics: {
		processed_bytes_total:  components.sources.internal_metrics.output.metrics.processed_bytes_total
		processed_events_total: components.sources.internal_metrics.output.metrics.processed_events_total
	}
}
