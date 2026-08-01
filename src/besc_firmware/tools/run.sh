# FL, FR, BL, BR
# 3050, 3060, 3070, 3080 respectively
yakut pub -N 1 \
    3080:reg.udral.service.actuator.common.sp.Scalar.0.1 "{value: 0.3}"
sleep 3
yakut pub -N 1 \
    3080:reg.udral.service.actuator.common.sp.Scalar.0.1 "{value: 0.0}"
