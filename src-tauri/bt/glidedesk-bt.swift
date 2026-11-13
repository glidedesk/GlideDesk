// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
//
// The Bluetooth side of a computer-to-computer connection.
//
// Two computers that are not on a network they share still have Bluetooth. One
// of them advertises a small service that carries the details of the network the
// other can join (the direct network this app hosts), so the other can set itself
// up without anyone typing a password. When there is no direct network to offer,
// the same channel still says where the host is, so the two can meet there.
//
// This is a command-line helper the app starts and stops; it prints one JSON
// object per run and exits. See transport/bt.rs for how it is used.

import CoreBluetooth
import Foundation

let serviceUUID = CBUUID(string: "6a1c7d10-9e42-4b1a-9c2e-676c69646564")
let characteristicUUID = CBUUID(string: "6a1c7d11-9e42-4b1a-9c2e-676c69646564")

// MARK: - Output

func say(_ object: [String: Any]) {
    let data = (try? JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])) ?? Data("{}".utf8)
    FileHandle.standardOutput.write(data)
    FileHandle.standardOutput.write(Data("\n".utf8))
}

func emit(_ object: [String: Any], code: Int32 = 0) -> Never {
    say(object)
    exit(code)
}

func emitError(_ message: String) -> Never {
    emit(["ok": false, "error": message], code: 1)
}

// MARK: - Arguments

var options: [String: String] = [:]
var mode = ""
var index = 1
while index < CommandLine.arguments.count {
    let arg = CommandLine.arguments[index]
    if arg.hasPrefix("--") {
        let key = String(arg.dropFirst(2))
        let value = index + 1 < CommandLine.arguments.count ? CommandLine.arguments[index + 1] : ""
        options[key] = value
        index += 2
    } else {
        mode = arg
        index += 1
    }
}

func seconds(_ fallback: Double) -> Double {
    Double(options["seconds"] ?? "") ?? fallback
}

// MARK: - State

final class Link: NSObject, CBCentralManagerDelegate, CBPeripheralDelegate, CBPeripheralManagerDelegate {
    var central: CBCentralManager?
    var peripheral: CBPeripheralManager?
    var found: [(id: UUID, name: String, rssi: Int, connectable: Bool)] = []
    var servicesSeen: [UUID: [String]] = [:]
    var wanted: UUID?
    var scanner: CBPeripheral?
    var reading: CBPeripheral?
    var payload: Data?
    var startResult: [String: Any] = [:]

    // MARK: central (joining)

    func startScanning() {
        central = CBCentralManager(delegate: self, queue: nil)
    }

    func centralManagerDidUpdateState(_ manager: CBCentralManager) {
        switch manager.state {
        case .poweredOn:
            // A plain scan lists everything, which is how the app checks that
            // Bluetooth really works before it offers it; a read looks for the
            // service alone.
            let services: [CBUUID]? = mode == "scanall" ? nil : [serviceUUID]
            manager.scanForPeripherals(withServices: services, options: [
                CBCentralManagerScanOptionAllowDuplicatesKey: false,
            ])
        case .unauthorized:
            emitError("Bluetooth access is not allowed for GlideDesk. Allow it in System Settings › Privacy & Security › Bluetooth.")
        case .unsupported:
            emitError("This Mac's Bluetooth does not support Bluetooth LE.")
        case .poweredOff:
            emitError("Bluetooth is off. Turn it on in System Settings › Bluetooth.")
        default:
            break
        }
    }

    func centralManager(_ manager: CBCentralManager, didDiscover peripheral: CBPeripheral,
                        advertisementData: [String: Any], rssi RSSI: NSNumber) {
        let name = (advertisementData[CBAdvertisementDataLocalNameKey] as? String)
            ?? peripheral.name
            ?? ""
        if !found.contains(where: { $0.id == peripheral.identifier }) {
            let services = (advertisementData[CBAdvertisementDataServiceUUIDsKey] as? [CBUUID] ?? [])
                .map { $0.uuidString }
            found.append((
                id: peripheral.identifier,
                name: name,
                rssi: RSSI.intValue,
                connectable: (advertisementData[CBAdvertisementDataIsConnectable] as? NSNumber)?.boolValue ?? true
            ))
            servicesSeen[peripheral.identifier] = services
        }
        // A scan only reports; a read picks one up and connects to it.
        if mode == "read" {
            let wantedID = options["id"].flatMap { UUID(uuidString: $0) }
            if wantedID == nil || wantedID == peripheral.identifier {
                wanted = peripheral.identifier
                scanner = peripheral
                peripheral.delegate = self
                manager.stopScan()
                manager.connect(peripheral, options: nil)
            }
        }
    }

    func centralManager(_ manager: CBCentralManager, didConnect peripheral: CBPeripheral) {
        peripheral.discoverServices([serviceUUID])
    }

    func centralManager(_ manager: CBCentralManager, didFailToConnect peripheral: CBPeripheral, error: Error?) {
        emitError("Could not connect over Bluetooth: \(error?.localizedDescription ?? "no answer")")
    }

    func peripheral(_ peripheral: CBPeripheral, didDiscoverServices error: Error?) {
        guard let service = peripheral.services?.first(where: { $0.uuid == serviceUUID }) else {
            emitError("The other computer's Bluetooth service was not there.")
        }
        peripheral.discoverCharacteristics([characteristicUUID], for: service)
    }

    func peripheral(_ peripheral: CBPeripheral, didDiscoverCharacteristicsFor service: CBService, error: Error?) {
        guard let characteristic = service.characteristics?.first(where: { $0.uuid == characteristicUUID }) else {
            emitError("The other computer's Bluetooth service had nothing to read.")
        }
        peripheral.readValue(for: characteristic)
    }

    func peripheral(_ peripheral: CBPeripheral, didUpdateValueFor characteristic: CBCharacteristic, error: Error?) {
        if let error = error {
            emitError("Could not read over Bluetooth: \(error.localizedDescription)")
        }
        guard let data = characteristic.value, !data.isEmpty else {
            emitError("The other computer sent nothing over Bluetooth.")
        }
        let text = String(data: data, encoding: .utf8) ?? ""
        var answer: [String: Any] = [
            "ok": true,
            "payload": data.base64EncodedString(),
            "text": text,
            "name": peripheral.name ?? "",
        ]
        if let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any] {
            answer["details"] = json
            if let name = json["name"] as? String { answer["name"] = name }
        }
        emit(answer)
    }

    // MARK: peripheral (hosting)

    func startAdvertising(payload: Data, name: String) {
        self.payload = payload
        self.startResult["name"] = name
        peripheral = CBPeripheralManager(delegate: self, queue: nil)
    }

    func peripheralManagerDidStartAdvertising(_ manager: CBPeripheralManager, error: Error?) {
        if let error = error {
            emitError("Could not advertise over Bluetooth: \(error.localizedDescription)")
        }
        // Say it is up, then keep serving reads until the app stops us.
        say(["ok": true, "advertising": true, "name": startResult["name"] ?? "GlideDesk"])
    }

    func peripheralManagerDidUpdateState(_ manager: CBPeripheralManager) {
        switch manager.state {
        case .poweredOn:
            let characteristic = CBMutableCharacteristic(
                type: characteristicUUID,
                properties: [.read],
                value: payload,
                permissions: [.readable]
            )
            let service = CBMutableService(type: serviceUUID, primary: true)
            service.characteristics = [characteristic]
            manager.add(service)
            manager.startAdvertising([
                CBAdvertisementDataServiceUUIDsKey: [serviceUUID],
                CBAdvertisementDataLocalNameKey: (startResult["name"] as? String) ?? "GlideDesk",
            ])
        case .unauthorized:
            emitError("Bluetooth access is not allowed for GlideDesk. Allow it in System Settings › Privacy & Security › Bluetooth.")
        case .unsupported:
            emitError("This Mac's Bluetooth does not support Bluetooth LE.")
        case .poweredOff:
            emitError("Bluetooth is off. Turn it on in System Settings › Bluetooth.")
        default:
            break
        }
    }
}

// MARK: - Run

let link = Link()

switch mode {
case "caps":
    link.startScanning()
    let deadline = Date().addingTimeInterval(3)
    while Date() < deadline {
        RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.1))
        if let state = link.central?.state {
            switch state {
            case .poweredOn:
                emit(["ok": true, "le": true, "state": "on"])
            case .poweredOff:
                emit(["ok": true, "le": true, "state": "off"])
            case .unauthorized:
                emit(["ok": true, "le": true, "state": "denied"])
            case .unsupported:
                emit(["ok": true, "le": false, "state": "unsupported"])
            default:
                continue
            }
        }
    }
    emit(["ok": true, "le": true, "state": "unknown"])

case "scan", "scanall":
    link.startScanning()
    RunLoop.main.run(until: Date().addingTimeInterval(seconds(6)))
    emit([
        "ok": true,
        "devices": link.found.map {
            ["id": $0.id.uuidString, "name": $0.name, "rssi": $0.rssi,
             "services": link.servicesSeen[$0.id] ?? []]
        },
    ])

case "read":
    link.startScanning()
    RunLoop.main.run(until: Date().addingTimeInterval(seconds(12)))
    if link.found.isEmpty {
        emitError("No computer was advertising a direct network nearby over Bluetooth.")
    }
    emitError("The other computer was seen but did not answer over Bluetooth.")

case "advertise":
    guard let encoded = options["payload"], let data = Data(base64Encoded: encoded), !data.isEmpty else {
        emitError("Nothing to advertise.")
    }
    link.startAdvertising(payload: data, name: options["name"] ?? "GlideDesk")
    RunLoop.main.run(until: Date.distantFuture)

default:
    emitError("Say caps, scan, scanall, read or advertise.")
}
