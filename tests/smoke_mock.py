"""Run after cargo build --release; exercise TUI/SDK with fake local AWS responses."""

import codecs
import fcntl
import json
import os
import pty
import select
import struct
import subprocess
import termios
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from threading import Thread
from urllib.parse import parse_qs


ACCOUNT = "123456789012"
MACHINE = f"arn:aws:states:us-east-1:{ACCOUNT}:stateMachine:smoke"
FUNCTION = f"arn:aws:lambda:eu-west-1:{ACCOUNT}:function:worker"
EXECUTION = f"arn:aws:states:us-east-1:{ACCOUNT}:execution:smoke:run"
TOPIC = f"arn:aws:sns:us-east-1:{ACCOUNT}:topic"
QUEUE = f"arn:aws:sqs:us-east-1:{ACCOUNT}:queue"
DEAD_QUEUE = f"arn:aws:sqs:us-east-1:{ACCOUNT}:dead"
API_FUNCTION = f"arn:aws:lambda:us-east-1:{ACCOUNT}:function:worker"
INTEGRATION = ("arn:aws:apigateway:us-east-1:lambda:path/2015-03-31/functions/"
               f"{API_FUNCTION}/invocations")
CALLS = []


def cbor(value):
    def prefix(kind, length):
        if length < 24:
            return bytes([kind | length])
        if length < 256:
            return bytes([kind | 24, length])
        return bytes([kind | 25]) + length.to_bytes(2, "big")

    if isinstance(value, str):
        encoded = value.encode()
        return prefix(0x60, len(encoded)) + encoded
    if isinstance(value, int):
        return prefix(0, value)
    if isinstance(value, list):
        return prefix(0x80, len(value)) + b"".join(map(cbor, value))
    if isinstance(value, dict):
        return prefix(0xa0, len(value)) + b"".join(
            cbor(key) + cbor(item) for key, item in value.items())
    raise TypeError(value)


class MockAws(BaseHTTPRequestHandler):
    def log_message(self, *_args):
        pass

    def do_POST(self):
        self.respond()

    def do_GET(self):
        self.respond()

    def respond(self):
        raw = self.rfile.read(int(self.headers.get("content-length", "0")))
        body = "" if self.headers.get("content-type") == "application/cbor" else raw.decode()
        operation = self.headers.get("x-amz-target", "") or self.path
        if operation == "/":
            operation = parse_qs(body).get("Action", [operation])[0]
        auth = self.headers.get("authorization", "")
        region = auth.split("Credential=")[-1].split("/")[2] if "Credential=" in auth else "?"
        CALLS.append((operation, region))
        if "Action=GetCallerIdentity" in body:
            payload = (f'<GetCallerIdentityResponse xmlns="https://sts.amazonaws.com/doc/2011-06-15/">'
                       f"<GetCallerIdentityResult><Account>{ACCOUNT}</Account>"
                       f"<Arn>arn:aws:iam::{ACCOUNT}:user/mock</Arn><UserId>MOCK</UserId>"
                       "</GetCallerIdentityResult></GetCallerIdentityResponse>").encode()
            content_type = "text/xml"
        elif operation.endswith("ListStateMachines"):
            payload = json.dumps({"stateMachines": [{"stateMachineArn": MACHINE,
                "name": "smoke", "type": "STANDARD", "creationDate": 1.0}]}).encode()
            content_type = "application/x-amz-json-1.0"
        elif operation.endswith("DescribeStateMachine"):
            definition = json.dumps({"StartAt": "Work", "States": {"Work": {
                "Type": "Task", "Resource": FUNCTION, "End": True}}})
            payload = json.dumps({"stateMachineArn": MACHINE, "name": "smoke",
                "type": "STANDARD", "roleArn": f"arn:aws:iam::{ACCOUNT}:role/mock",
                "definition": definition, "creationDate": 1.0}).encode()
            content_type = "application/x-amz-json-1.0"
        elif operation.endswith("ListExecutions"):
            payload = json.dumps({"executions": [{"executionArn": EXECUTION,
                "stateMachineArn": MACHINE, "name": "run", "status": "SUCCEEDED",
                "startDate": 1.0}]}).encode()
            content_type = "application/x-amz-json-1.0"
        elif operation.endswith("DescribeExecution"):
            payload = json.dumps({"executionArn": EXECUTION, "stateMachineArn": MACHINE,
                "status": "SUCCEEDED", "startDate": 1.0, "input": "{}"}).encode()
            content_type = "application/x-amz-json-1.0"
        elif operation.endswith("GetExecutionHistory"):
            payload = json.dumps({"events": [{"timestamp": 1.0, "type": "ExecutionStarted",
                "id": 1}]}).encode()
            content_type = "application/x-amz-json-1.0"
        elif operation.split("?")[0] == "/2015-03-31/functions":
            payload = json.dumps({"Functions": [{"FunctionName": "worker",
                "FunctionArn": API_FUNCTION, "Runtime": "nodejs22.x"}]}).encode()
            content_type = "application/json"
        elif operation.endswith("/configuration"):
            payload = json.dumps({"FunctionName": "worker", "FunctionArn": FUNCTION,
                "Role": f"arn:aws:iam::{ACCOUNT}:role/mock",
                "Environment": {"Variables": {"SECRET_NAME": "do-not-show"}}}).encode()
            content_type = "application/json"
        elif "/event-source-mappings" in operation:
            payload = json.dumps({"EventSourceMappings": [{"UUID": "mock-mapping",
                "EventSourceArn": f"arn:aws:sqs:eu-west-1:{ACCOUNT}:queue",
                "State": "Enabled"}]}).encode()
            content_type = "application/json"
        elif operation.endswith("/hostedzone"):
            payload = (b'<ListHostedZonesResponse xmlns="https://route53.amazonaws.com/doc/2013-04-01/">'
                b'<HostedZones><HostedZone><Id>/hostedzone/ZMOCK</Id><Name>example.test.</Name>'
                b'<CallerReference>mock</CallerReference><Config><PrivateZone>false</PrivateZone>'
                b'</Config></HostedZone></HostedZones><IsTruncated>false</IsTruncated>'
                b'</ListHostedZonesResponse>')
            content_type = "text/xml"
        elif "/hostedzone/" in operation and "/rrset" in operation:
            payload = (b'<ListResourceRecordSetsResponse xmlns="https://route53.amazonaws.com/doc/2013-04-01/">'
                b'<ResourceRecordSets><ResourceRecordSet><Name>example.test.</Name><Type>A</Type>'
                b'<TTL>60</TTL><ResourceRecords><ResourceRecord><Value>192.0.2.1</Value>'
                b'</ResourceRecord></ResourceRecords></ResourceRecordSet></ResourceRecordSets>'
                b'<IsTruncated>false</IsTruncated></ListResourceRecordSetsResponse>')
            content_type = "text/xml"
        elif "/hostedzone/" in operation:
            payload = (b'<GetHostedZoneResponse xmlns="https://route53.amazonaws.com/doc/2013-04-01/">'
                b'<HostedZone><Id>/hostedzone/ZMOCK</Id><Name>example.test.</Name>'
                b'<CallerReference>mock</CallerReference><Config><PrivateZone>false</PrivateZone>'
                b'</Config></HostedZone></GetHostedZoneResponse>')
            content_type = "text/xml"
        elif "Action=ListTopics" in body:
            payload = (f'<ListTopicsResponse xmlns="http://sns.amazonaws.com/doc/2010-03-31/">'
                f'<ListTopicsResult><Topics><member><TopicArn>{TOPIC}</TopicArn></member>'
                '</Topics></ListTopicsResult></ListTopicsResponse>').encode()
            content_type = "text/xml"
        elif "Action=GetTopicAttributes" in body:
            payload = (f'<GetTopicAttributesResponse xmlns="http://sns.amazonaws.com/doc/2010-03-31/">'
                f'<GetTopicAttributesResult><Attributes><entry><key>TopicArn</key>'
                f'<value>{TOPIC}</value></entry></Attributes></GetTopicAttributesResult>'
                '</GetTopicAttributesResponse>').encode()
            content_type = "text/xml"
        elif "Action=ListSubscriptionsByTopic" in body:
            payload = (f'<ListSubscriptionsByTopicResponse xmlns="http://sns.amazonaws.com/doc/2010-03-31/">'
                f'<ListSubscriptionsByTopicResult><Subscriptions><member><TopicArn>{TOPIC}</TopicArn>'
                f'<SubscriptionArn>{TOPIC}:sub</SubscriptionArn><Protocol>sqs</Protocol>'
                f'<Endpoint>{QUEUE}</Endpoint></member></Subscriptions>'
                '</ListSubscriptionsByTopicResult></ListSubscriptionsByTopicResponse>').encode()
            content_type = "text/xml"
        elif "ListQueues" in operation or '"Action":"ListQueues"' in body:
            payload = json.dumps({"QueueUrls": [f"https://sqs.us-east-1.amazonaws.com/{ACCOUNT}/queue"]}).encode()
            content_type = "application/json"
        elif "GetQueueAttributes" in operation or '"Action":"GetQueueAttributes"' in body:
            queue_arn = DEAD_QUEUE if "dead" in body else QUEUE
            attributes = {"QueueArn": queue_arn, "ApproximateNumberOfMessages": "0"}
            if queue_arn == QUEUE:
                attributes["RedrivePolicy"] = json.dumps({"deadLetterTargetArn": DEAD_QUEUE})
            payload = json.dumps({"Attributes": attributes}).encode()
            content_type = "application/json"
        elif operation.endswith("ListEventBuses"):
            payload = json.dumps({"EventBuses": [{"Name": "default",
                "Arn": f"arn:aws:events:us-east-1:{ACCOUNT}:event-bus/default"}]}).encode()
            content_type = "application/x-amz-json-1.1"
        elif operation.endswith("DescribeEventBus"):
            payload = json.dumps({"Name": "default",
                "Arn": f"arn:aws:events:us-east-1:{ACCOUNT}:event-bus/default"}).encode()
            content_type = "application/x-amz-json-1.1"
        elif operation.endswith("ListRules"):
            payload = json.dumps({"Rules": [{"Name": "smoke", "State": "ENABLED",
                "Arn": f"arn:aws:events:us-east-1:{ACCOUNT}:rule/default/smoke"}]}).encode()
            content_type = "application/x-amz-json-1.1"
        elif operation.endswith("DescribeRule"):
            payload = json.dumps({"Name": "smoke", "State": "ENABLED",
                "Arn": f"arn:aws:events:us-east-1:{ACCOUNT}:rule/default/smoke"}).encode()
            content_type = "application/x-amz-json-1.1"
        elif operation.endswith("ListTargetsByRule"):
            payload = json.dumps({"Targets": [{"Id": "queue-target", "Arn": QUEUE}]}).encode()
            content_type = "application/x-amz-json-1.1"
        elif operation.endswith("DescribeAlarms"):
            payload = cbor({"MetricAlarms": [{"AlarmName": "smoke-alarm",
                "AlarmArn": f"arn:aws:cloudwatch:us-east-1:{ACCOUNT}:alarm:smoke-alarm",
                "StateValue": "OK", "MetricName": "Invocations",
                "Namespace": "AWS/Lambda"}]})
            content_type = "application/cbor"
        elif operation.endswith("ListMetrics"):
            payload = cbor({"Metrics": [{"MetricName": "Invocations",
                "Namespace": "AWS/Lambda", "Dimensions": [{"Name": "FunctionName",
                "Value": "worker"}]}]})
            content_type = "application/cbor"
        elif operation.endswith("DescribeLogGroups"):
            payload = json.dumps({"logGroups": [{"logGroupName": "/aws/lambda/worker",
                "arn": f"arn:aws:logs:us-east-1:{ACCOUNT}:log-group:/aws/lambda/worker"}]}).encode()
            content_type = "application/x-amz-json-1.1"
        elif operation.endswith("DescribeLogStreams"):
            payload = json.dumps({"logStreams": [{"logStreamName": "stream-1"}]}).encode()
            content_type = "application/x-amz-json-1.1"
        elif operation.endswith("GetLogEvents"):
            request = json.loads(body)
            assert request["limit"] == 100
            next_token = "cursor2" if "nextToken" not in request else request["nextToken"]
            payload = json.dumps({"events": [{"timestamp": 1, "message": "mock log event"}],
                "nextForwardToken": next_token}).encode()
            content_type = "application/x-amz-json-1.1"
        elif operation.split("?")[0] == "/restapis":
            payload = json.dumps({"item": [{"id": "api1", "name": "rest-smoke"}]}).encode()
            content_type = "application/json"
        elif operation.split("?")[0] == "/restapis/api1":
            payload = json.dumps({"id": "api1", "name": "rest-smoke"}).encode()
            content_type = "application/json"
        elif operation.split("?")[0] == "/restapis/api1/resources":
            payload = json.dumps({"item": [{"id": "resource1", "path": "/test",
                "resourceMethods": {"GET": {}}}]}).encode()
            content_type = "application/json"
        elif operation.split("?")[0] == "/restapis/api1/stages":
            payload = json.dumps({"item": [{"stageName": "prod"}]}).encode()
            content_type = "application/json"
        elif "/restapis/api1/resources/resource1/methods/GET/integration" in operation:
            payload = json.dumps({"type": "AWS", "uri": INTEGRATION}).encode()
            content_type = "application/json"
        elif "/restapis/api1/resources/resource1/methods/GET" in operation:
            payload = json.dumps({"httpMethod": "GET", "authorizationType": "NONE",
                "methodIntegration": {"type": "AWS", "uri": INTEGRATION}}).encode()
            content_type = "application/json"
        elif operation.split("?")[0] == "/v2/apis":
            payload = json.dumps({"items": [
                {"apiId": "http1", "name": "http-smoke", "protocolType": "HTTP"},
                {"apiId": "ws1", "name": "ws-smoke", "protocolType": "WEBSOCKET"}]}).encode()
            content_type = "application/json"
        elif operation.split("?")[0] in ("/v2/apis/http1", "/v2/apis/ws1"):
            family = "WEBSOCKET" if "/ws1" in operation else "HTTP"
            payload = json.dumps({"apiId": "ws1" if family == "WEBSOCKET" else "http1",
                "name": "mock-api", "protocolType": family,
                "apiEndpoint": "https://example.test"}).encode()
            content_type = "application/json"
        elif operation.split("?")[0].endswith("/routes"):
            payload = json.dumps({"items": [{"routeId": "r1", "routeKey": "GET /test",
                "target": "integrations/i1"}]}).encode()
            content_type = "application/json"
        elif operation.split("?")[0].endswith("/integrations"):
            payload = json.dumps({"items": [{"integrationId": "i1",
                "integrationType": "AWS_PROXY", "integrationUri": API_FUNCTION}]}).encode()
            content_type = "application/json"
        elif operation.split("?")[0].endswith("/stages"):
            payload = json.dumps({"items": [{"stageName": "$default",
                "autoDeploy": True}]}).encode()
            content_type = "application/json"
        else:
            raise AssertionError(f"Unexpected AWS operation: {operation}")
        self.send_response(200)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)


class Screen:
    def __init__(self, master, rows=38, columns=120):
        self.master = master
        self.cells = [[" "] * columns for _ in range(rows)]
        self.row = self.col = 0
        self.mode = "text"
        self.args = ""
        self.decoder = codecs.getincrementaldecoder("utf8")()

    def feed(self, data):
        for char in self.decoder.decode(data):
            if self.mode == "esc":
                self.mode = "csi" if char == "[" else "text"
                self.args = ""
                continue
            if self.mode == "csi":
                if char.isalpha():
                    numbers = [int(value or "0") for value in self.args.lstrip("?").split(";")
                               if value == "" or value.isdigit()]
                    if char in "Hf":
                        self.row = max(0, (numbers[0] if numbers else 1) - 1)
                        self.col = max(0, (numbers[1] if len(numbers) > 1 else 1) - 1)
                    elif char == "G":
                        self.col = max(0, (numbers[0] if numbers else 1) - 1)
                    elif char == "J" and (not numbers or numbers[0] == 2):
                        for line in self.cells:
                            line[:] = [" "] * len(line)
                    elif char == "K" and self.row < len(self.cells):
                        width = len(self.cells[0])
                        self.cells[self.row][min(self.col, width):] = [" "] * (width - min(self.col, width))
                    self.mode = "text"
                else:
                    self.args += char
            elif char == "\x1b":
                self.mode = "esc"
            elif char == "\r":
                self.col = 0
            elif char == "\n":
                self.row = min(self.row + 1, len(self.cells) - 1)
            elif char >= " " and self.row < len(self.cells) and self.col < len(self.cells[0]):
                self.cells[self.row][self.col] = char
                self.col += 1

    def line(self, row):
        return "".join(self.cells[row]).strip()

    def wait(self, condition, timeout=5):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            ready, _, _ = select.select([self.master], [], [], 0.1)
            if ready:
                data = os.read(self.master, 65536)
                if not data:
                    raise AssertionError("App closed before expected screen update: " + repr(CALLS))
                self.feed(data)
            if condition():
                return
        raise AssertionError("Timed out; screen: " + repr([self.line(index)[:100]
                             for index in (1, 3, 4, 5, len(self.cells) - 5, len(self.cells) - 4)])
                             + "; calls: " + repr(CALLS))

    def key(self, text):
        os.write(self.master, text.encode())
        deadline = time.monotonic() + 0.2
        while time.monotonic() < deadline:
            ready, _, _ = select.select([self.master], [], [], 0.05)
            if ready:
                data = os.read(self.master, 65536)
                if not data:
                    break
                self.feed(data)

def check_narrow(env, rows, columns):
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", rows, columns, 0, 0))
    process = subprocess.Popen(["target/release/a6i", "--region", "us-east-1"],
                               stdin=slave, stdout=slave, stderr=slave, env=env)
    os.close(slave)
    screen = Screen(master, rows, columns)
    try:
        screen.wait(lambda: ACCOUNT in screen.line(1))
        screen.wait(lambda: "? help" in screen.line(rows - 2))
        screen.key("?")
        screen.wait(lambda: ":exit   :quit" in "\n".join(map(screen.line, range(rows))))
        help_text = "\n".join(map(screen.line, range(rows)))
        for command in (":route53", ":apigateway", ":lambda", ":stepfunctions",
                        ":cloudwatch", ":sns", ":sqs", ":eventbridge",
                        ":profile NAME", ":region REGION", "PgUp/PgDn"):
            assert command in help_text, (rows, columns, command, help_text)
        screen.key("\x1b")
        screen.wait(lambda: "Services" in screen.line(3))
        screen.key(":stepfunctions\r")
        screen.wait(lambda: "stepfunctions | 1 rows | Complete" in screen.line(3))
        assert "Enter: full detail" in screen.line(3)
        screen.key("/missing\r")
        screen.wait(lambda: "0/1 rows" in screen.line(3))
        screen.wait(lambda: "No matches" in "\n".join(map(screen.line, range(rows)))
                    and "loaded rows" in "\n".join(map(screen.line, range(rows))))
        screen.key("/\x7f\x7f\x7f\x7f\x7f\x7f\x7f\r")
        screen.wait(lambda: "stepfunctions | 1 rows | Complete" in screen.line(3))
        screen.key("\r")
        screen.wait(lambda: "definition:" in "\n".join(map(screen.line, range(rows))))
        assert "PgUp/PgDn scroll" in screen.line(rows - 2)
        screen.key("\x1b[6~")
        screen.wait(lambda: '"Type": "Task"' in "\n".join(map(screen.line, range(rows))))
        screen.key(":quit\r")
        process.wait(timeout=4)
        assert process.returncode == 0
    finally:
        if process.poll() is None:
            process.terminate()
            process.wait(timeout=4)
        os.close(master)


def run():
    server = ThreadingHTTPServer(("127.0.0.1", 0), MockAws)
    Thread(target=server.serve_forever, daemon=True).start()
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 38, 120, 0, 0))
    env = dict(os.environ, AWS_ACCESS_KEY_ID="mock", AWS_SECRET_ACCESS_KEY="mock",
               AWS_REGION="us-east-1", AWS_ENDPOINT_URL=f"http://127.0.0.1:{server.server_port}",
               AWS_CONFIG_FILE="/dev/null", AWS_SHARED_CREDENTIALS_FILE="/dev/null",
               AWS_EC2_METADATA_DISABLED="true", TERM="xterm-256color")
    for name in ("AWS_PROFILE", "AWS_DEFAULT_PROFILE", "AWS_SESSION_TOKEN"):
        env.pop(name, None)
    process = subprocess.Popen(["target/release/a6i", "--region", "us-east-1"],
                               stdin=slave, stdout=slave, stderr=slave, env=env)
    os.close(slave)
    screen = Screen(master)
    try:
        screen.wait(lambda: ACCOUNT in screen.line(1))
        screen.key(":stepfunctions\r")
        screen.wait(lambda: "stepfunctions | 1 rows | Complete" in screen.line(3))
        screen.key("\r")
        screen.wait(lambda: "function:" in screen.line(5))
        screen.key("\r")
        screen.wait(lambda: any(op.endswith("ListExecutions") for op, _ in CALLS))
        screen.wait(lambda: "1 rows | Complete" in screen.line(3))
        screen.key("\r")
        screen.wait(lambda: any(op.endswith("DescribeExecution") for op, _ in CALLS))
        screen.wait(lambda: "history:" in screen.line(4))
        screen.key("\r")
        screen.wait(lambda: any(op.endswith("GetExecutionHistory") for op, _ in CALLS))
        screen.wait(lambda: "1 rows | Complete" in screen.line(3))
        screen.key("\x1b")
        screen.key("\x1b")
        screen.key("\x1b")
        screen.wait(lambda: "function:" in screen.line(5))
        screen.key("j")
        screen.key("\r")
        screen.wait(lambda: "Switch to eu-west-1" in screen.line(34))
        screen.key("\r")
        screen.wait(lambda: any(op.endswith("/configuration") for op, _ in CALLS))
        assert CALLS[-1][1] == "eu-west-1", CALLS
        screen.wait(lambda: "mappings:" in screen.line(4))
        assert "do-not-show" not in "\n".join(screen.line(index) for index in range(38))
        screen.key("\r")
        screen.wait(lambda: any("/event-source-mappings" in op for op, _ in CALLS))
        screen.key("\x1b")
        screen.key("\x1b")
        screen.wait(lambda: "region: us-east-1" in screen.line(1))
        screen.key(":route53\r")
        screen.wait(lambda: "route53 | 1 rows | Complete" in screen.line(3))
        screen.key("\r")
        screen.wait(lambda: any("/hostedzone/" in op and "/rrset" not in op
                                for op, _ in CALLS))
        screen.wait(lambda: "records:" in screen.line(4))
        screen.key("\r")
        screen.wait(lambda: any("/rrset" in op for op, _ in CALLS))
        screen.wait(lambda: "route53 | 1 rows | Complete" in screen.line(3))
        screen.key(":sns\r")
        screen.wait(lambda: "sns | 1 rows | Complete" in screen.line(3))
        screen.key("\r")
        screen.wait(lambda: "subscriptions:" in screen.line(4))
        screen.key("\r")
        screen.wait(lambda: any("ListSubscriptionsByTopic" in op for op, _ in CALLS))
        screen.wait(lambda: "sns | 1 rows | Complete" in screen.line(3))
        screen.key("\r")
        screen.wait(lambda: "queue:" in screen.line(4))
        screen.key("\r")
        screen.wait(lambda: any("GetQueueAttributes" in op for op, _ in CALLS))
        screen.wait(lambda: "> queue:" in screen.line(4))
        screen.key("\r")
        screen.wait(lambda: sum("GetQueueAttributes" in op for op, _ in CALLS) >= 2)
        screen.key(":sqs\r")
        screen.wait(lambda: "sqs | 1 rows | Complete" in screen.line(3))
        screen.key(":eventbridge\r")
        screen.wait(lambda: "eventbridge | 1 rows | Complete" in screen.line(3))
        screen.key("\r")
        screen.wait(lambda: any(op.endswith("DescribeEventBus") for op, _ in CALLS))
        screen.wait(lambda: "rules:" in screen.line(4))
        screen.key("\r")
        screen.wait(lambda: any(op.endswith("ListRules") for op, _ in CALLS))
        screen.wait(lambda: "eventbridge | 1 rows | Complete" in screen.line(3))
        screen.key("\r")
        screen.wait(lambda: any(op.endswith("DescribeRule") for op, _ in CALLS))
        screen.wait(lambda: "targets:" in screen.line(4))
        screen.key("\r")
        screen.wait(lambda: any(op.endswith("ListTargetsByRule") for op, _ in CALLS))
        screen.wait(lambda: "eventbridge | 1 rows | Complete" in screen.line(3))
        screen.key("\r")
        screen.wait(lambda: "queue:" in screen.line(4))
        before = sum("GetQueueAttributes" in op for op, _ in CALLS)
        screen.key("\r")
        screen.wait(lambda: sum("GetQueueAttributes" in op for op, _ in CALLS) > before)
        screen.key(":apigateway\r")
        screen.wait(lambda: "apigateway | 3 rows | Complete" in screen.line(3))
        screen.key("\r")
        screen.key("\r")
        screen.wait(lambda: "apigateway | 1 rows | Complete" in screen.line(3))
        screen.key("\r")
        screen.wait(lambda: "/restapis/api1" in [op for op, _ in CALLS])
        screen.wait(lambda: "rest_resources:" in screen.line(4))
        screen.key("\r")
        screen.wait(lambda: any("/restapis/api1/resources" in op for op, _ in CALLS))
        screen.wait(lambda: "apigateway | 1 rows | Complete" in screen.line(3))
        screen.key("\r")
        screen.wait(lambda: "rest_method:" in screen.line(4))
        screen.key("\r")
        screen.wait(lambda: any("/methods/GET/integration" in op for op, _ in CALLS))
        screen.wait(lambda: "function:" in screen.line(4))
        before = sum(op.endswith("/configuration") for op, _ in CALLS)
        screen.key("\r")
        screen.wait(lambda: sum(op.endswith("/configuration") for op, _ in CALLS) > before)
        screen.key(":apigateway\r")
        screen.wait(lambda: "apigateway | 3 rows | Complete" in screen.line(3))
        screen.key("\r\r")
        screen.wait(lambda: "apigateway | 1 rows | Complete" in screen.line(3))
        screen.key("\r")
        screen.wait(lambda: "rest_stages:" in screen.line(5))
        screen.key("j\r")
        screen.wait(lambda: any(op.split("?")[0] == "/restapis/api1/stages"
                                for op, _ in CALLS))
        screen.wait(lambda: "apigateway | 1 rows | Complete" in screen.line(3))
        for index, api_id in ((1, "http1"), (2, "ws1")):
            screen.key(":apigateway\r")
            screen.wait(lambda: "apigateway | 3 rows | Complete" in screen.line(3))
            screen.key("j" * index)
            screen.key("\r")
            screen.key("\r")
            screen.wait(lambda: "apigateway | 1 rows | Complete" in screen.line(3))
            screen.key("\r")
            screen.wait(lambda: any(op == f"/v2/apis/{api_id}" for op, _ in CALLS))
            screen.wait(lambda: "routes:" in screen.line(4))
            screen.key("\r")
            screen.wait(lambda: any(f"/v2/apis/{api_id}/routes" in op for op, _ in CALLS))
            screen.wait(lambda: "apigateway | 1 rows | Complete" in screen.line(3))
            screen.key("\x1b")
            screen.key("j")
            screen.key("\r")
            screen.wait(lambda: any(f"/v2/apis/{api_id}/integrations" in op for op, _ in CALLS))
            screen.wait(lambda: "apigateway | 1 rows | Complete" in screen.line(3))
            screen.key("\r")
            screen.wait(lambda: "function:" in screen.line(4))
            screen.key("\x1b")
            screen.key("\x1b")
            screen.key("j")
            screen.key("\r")
            screen.wait(lambda: any(f"/v2/apis/{api_id}/stages" in op for op, _ in CALLS))
        screen.key(":cloudwatch\r")
        screen.wait(lambda: "cloudwatch | 3 rows | Complete" in screen.line(3))
        screen.key("\r\r")
        screen.wait(lambda: any(op.endswith("DescribeAlarms") for op, _ in CALLS))
        screen.wait(lambda: "cloudwatch | 1 rows | Complete" in screen.line(3))
        screen.key("\r")
        screen.wait(lambda: "smoke-alarm" in screen.line(3))
        screen.key(":cloudwatch\r")
        screen.wait(lambda: "cloudwatch | 3 rows | Complete" in screen.line(3))
        screen.key("j\r\r")
        screen.wait(lambda: any(op.endswith("DescribeLogGroups") for op, _ in CALLS))
        screen.wait(lambda: "cloudwatch | 1 rows | Complete" in screen.line(3))
        screen.key("\r")
        screen.wait(lambda: "log_streams:" in screen.line(4))
        screen.key("\r")
        screen.wait(lambda: any(op.endswith("DescribeLogStreams") for op, _ in CALLS))
        screen.wait(lambda: "cloudwatch | 1 rows | Complete" in screen.line(3))
        screen.key("\r")
        screen.wait(lambda: "log_events:" in screen.line(4))
        screen.key("\r")
        screen.wait(lambda: any(op.endswith("GetLogEvents") for op, _ in CALLS))
        screen.wait(lambda: "cloudwatch | 1 rows | Partial" in screen.line(3))
        screen.key("n")
        screen.wait(lambda: sum(op.endswith("GetLogEvents") for op, _ in CALLS) == 2)
        screen.wait(lambda: "cloudwatch | 2 rows | Complete" in screen.line(3))
        screen.key(":cloudwatch\r")
        screen.wait(lambda: "cloudwatch | 3 rows | Complete" in screen.line(3))
        screen.key("jj\r\r")
        screen.wait(lambda: any(op.endswith("ListMetrics") for op, _ in CALLS))
        screen.wait(lambda: "cloudwatch | 1 rows | Complete" in screen.line(3))
        screen.key("\r")
        screen.wait(lambda: any("FunctionName" in screen.line(index)
                                for index in range(3, 15)))
        screen.key(":lambda\r")
        screen.wait(lambda: any(op.split("?")[0] == "/2015-03-31/functions"
                                for op, _ in CALLS))
        screen.wait(lambda: "lambda | 1 rows | Complete" in screen.line(3))
        screen.key("\r")
        screen.wait(lambda: "mappings:" in screen.line(4))
        screen.key(":quit\r")
        process.wait(timeout=4)
        assert process.returncode == 0
        for rows, columns in ((24, 80), (20, 60)):
            check_narrow(env, rows, columns)
        print("Step Functions, Lambda, Route 53, SNS, SQS, EventBridge, API Gateway, CloudWatch: passed")
    finally:
        if process.poll() is None:
            process.terminate()
            process.wait(timeout=4)
        os.close(master)
        server.shutdown()
        server.server_close()


if __name__ == "__main__":
    run()
